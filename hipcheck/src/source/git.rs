//! Git related types and implementations for pulling/cloning source repos.

use crate::error::{Error as HcError, Result as HcResult};
use crate::{
	context::Context,
	shell::{progress_phase::ProgressPhase, Shell},
};
use console::Term;
use git2::{
	build::{CheckoutBuilder, RepoBuilder},
	FetchOptions, Progress, Reference, RemoteCallbacks, Repository,
};
use std::io::Write;
use std::{cell::OnceCell, path::Path};
use url::Url;

/// Construct the remote callbacks object uesd when making callinging into [git2].
fn make_remote_callbacks() -> RemoteCallbacks<'static> {
	// Create progress phases for recieving the objects and resolving deltas.
	let transfer_phase: OnceCell<ProgressPhase> = OnceCell::new();
	let resolution_phase: OnceCell<ProgressPhase> = OnceCell::new();

	// Create a struct to hold the callbacks.
	let mut callbacks = RemoteCallbacks::new();

	// Messages from the remote ("Counting objects" etc) are sent over the sideband.
	// This involves clearing and replacing the line -- use console to do this effectively.
	callbacks.sideband_progress(move |msg: &[u8]| {
		Shell::in_suspend(|| {
			// use the standard output.
			let mut term = Term::stdout();

			// Crash on errors here, since they should be relatively uncommon.
			term.clear_line().expect("clear line on standard output");

			write!(&mut term, "remote: {}", String::from_utf8_lossy(msg))
				.expect("wrote to standard output");

			term.flush().expect("flushed standard output");
		});

		true
	});

	callbacks.transfer_progress(move |prog: Progress| {
		if prog.received_objects() > 0 {
			let phase = transfer_phase.get_or_init(|| {
				ProgressPhase::start(prog.total_objects() as u64, "(git) recieving objects")
			});

			phase.set_position(prog.received_objects() as u64);

			if prog.received_objects() == prog.total_objects() && !phase.is_finished() {
				phase.finish_successful(false);
			}
		}

		if prog.indexed_deltas() > 0 {
			let phase = resolution_phase.get_or_init(|| {
				ProgressPhase::start(prog.total_deltas() as u64, "(git) resolving deltas")
			});

			phase.set_position(prog.indexed_deltas() as u64);

			if prog.indexed_deltas() == prog.total_deltas() && !phase.is_finished() {
				phase.finish_successful(false);
			}
		}

		true
	});

	callbacks
}

/// Build fetch options for [git2] using [make_remote_callbacks].
fn make_fetch_opts() -> FetchOptions<'static> {
	let mut fetch_opts = FetchOptions::new();

	fetch_opts
		// Use the remote callbacks for transfer.
		.remote_callbacks(make_remote_callbacks())
		// Don't download any tags.
		.download_tags(git2::AutotagOption::None);

	fetch_opts
}

fn make_checkout_builder() -> CheckoutBuilder<'static> {
	// Create a struct to hold callbacks while doing git checkout.
	let mut checkout_opts = CheckoutBuilder::new();

	// Make a phase to track the checkout progress.
	let checkout_phase: OnceCell<ProgressPhase> = OnceCell::new();

	// We don't care about the path being resolved, only the total and current numbers.
	checkout_opts.progress(move |path, current, total| {
		// Initialize the phase if we haven't already.
		let phase =
			checkout_phase.get_or_init(|| ProgressPhase::start(total as u64, "(git) checkout"));

		// Set the bar to have the amount of progress in resolving.
		phase.set_position(current as u64);
		// Set the progress bar's status to the path being resolved.
		phase.update_status(
			path.map(Path::to_string_lossy)
				.unwrap_or("resolving...".into()),
		);

		// If we have resolved everything, finish the phase.
		if current == total {
			phase.finish_successful(false);
		}
	});

	checkout_opts
}

/// Clone a repo from the given url to a destination path in the filesystem.
pub fn clone(url: &Url, dest: &Path) -> HcResult<()> {
	log::debug!("remote repository cloning url is {}", url);

	RepoBuilder::new()
		.with_checkout(make_checkout_builder())
		.fetch_options(make_fetch_opts())
		.clone(url.as_str(), dest)?;

	Ok(())
}

/// Update a repo in the filesystem at a given location.
///
/// This only supports fast-forwarding, and does so with no regard to the user's prefs.
/// Use with care, as that's not desireable in user facing repos (outside of the hipcheck cache).
pub fn update(repo_path: &Path) -> HcResult<()> {
	// Open the repo with git2.
	let repo: Repository = Repository::open(repo_path)?;
	// Get the repo's head.
	let head: Reference = repo.head()?;
	// Get the shortname for later debugging.
	let short_name = head
		.shorthand()
		.ok_or(HcError::msg("HEAD shorthand should be UTF-8"))?;
	// Get the refname of head.
	let refname = head.name().ok_or(HcError::msg(
		"Head name should be something like 'refs/heads/master'",
	))?;
	let mut local_branch_ref = repo.find_reference(refname)?;
	// Get the name of the remote that the local HEAD branch is tracking.
	let remote_name = repo.branch_upstream_remote(refname)?;
	let remote_name_str: &str = remote_name
		.as_str()
		.ok_or(HcError::msg("Remote name should be UTF-8"))?;
	// Find the remote object itself.
	let mut remote = repo.find_remote(remote_name_str)?;

	log::info!(
		"Fetched current refs for {refname} in {}",
		repo_path.display()
	);

	// Fetch the updated remote.
	remote.fetch(&[refname], Some(&mut make_fetch_opts()), None)?;

	// Get the commit we just fetched to.
	let fetch_head: Reference = repo.find_reference("FETCH_HEAD").context(format!(
		"Error finding FETCH_HEAD on {}",
		repo_path.display()
	))?;

	// Get the annotated commit to merge.
	let target_commit = repo
		.reference_to_annotated_commit(&fetch_head)
		.context("Error creating annotated commit")?;

	let reflog_msg = format!("Fast-forward {short_name} to id: {}", target_commit.id());

	// Set the local branch to the given commit
	local_branch_ref.set_target(target_commit.id(), &reflog_msg)?;

	// Update head and checkout.
	repo.set_head(refname)?;
	// Checkout with force.
	repo.checkout_head(Some(make_checkout_builder().force()))?;

	Ok(())
}
