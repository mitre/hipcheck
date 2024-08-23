//! Git related types and implementations for pulling/cloning source repos.

use crate::error::{Error as HcError, Result as HcResult};
use crate::shell::verbosity::Verbosity;
use crate::{
	context::Context,
	shell::{progress_phase::ProgressPhase, Shell},
};
use console::Term;
use git2::{
	build::{CheckoutBuilder, RepoBuilder},
	AnnotatedCommit, Branch, FetchOptions, Progress, Reference, RemoteCallbacks, Repository,
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

	match Shell::get_verbosity() {
		Verbosity::Normal => {
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
		}
		Verbosity::Quiet | Verbosity::Silent => {}
	}

	callbacks.transfer_progress(move |prog: Progress| {
		if prog.received_objects() > 0 {
			let phase = transfer_phase.get_or_init(|| {
				ProgressPhase::start(prog.total_objects() as u64, "(git) receiving objects")
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

/// For a given repo, checkout a particular ref in a detached HEAD state. If no
/// ref is provided, instead try to resolve the most correct ref to target. If
/// the repo has one branch, try fast-forwarding to match upstream, then set HEAD
/// to top of branch. Else, if the repo has one remote, try to find a local branch
/// tracking the default branch of remote and set HEAD to that. Otherwise, error.
pub fn checkout(repo_path: &Path, refspec: Option<String>) -> HcResult<String> {
	// Open the repo with git2.
	let repo: Repository = Repository::open(repo_path)?;
	// Get the repo's head.
	let head: Reference = repo.head()?;
	// Get the shortname for later debugging.
	let init_short_name = head
		.shorthand()
		.ok_or(HcError::msg("HEAD shorthand should be UTF-8"))?;
	let ret_str: String;
	if let Some(refspec_str) = refspec {
		// Parse refspec as an annotated commit, and set HEAD based on that
		let tgt_ref: AnnotatedCommit =
			repo.find_annotated_commit(repo.revparse_single(&refspec_str)?.peel_to_commit()?.id())?;
		repo.set_head_detached_from_annotated(tgt_ref)?;
		ret_str = refspec_str;
	} else {
		// Get names of remotes
		let raw_remotes = repo.remotes()?;
		let remotes = raw_remotes.into_iter().flatten().collect::<Vec<&str>>();
		let mut local_branches = repo
			.branches(Some(git2::BranchType::Local))?
			.filter_map(|x| match x {
				Ok((b, _)) => Some(b),
				_ => None,
			})
			.collect::<Vec<Branch>>();
		if local_branches.len() == 1 {
			let mut local_branch = local_branches.remove(0);
			// if applicable, update local_branch reference to match newest remote commit
			if let Ok(upstr) = local_branch.upstream() {
				let remote_ref = upstr.into_reference();
				let target_commit = repo
					.reference_to_annotated_commit(&remote_ref)
					.context("Error creating annotated commit")?;
				let reflog_msg = format!(
					"Fast-forward {init_short_name} to id: {}",
					target_commit.id()
				);
				// Set the local branch to the given commit
				local_branch
					.get_mut()
					.set_target(target_commit.id(), &reflog_msg)?;
			}
			// Get branch name in form "refs/heads/<NAME>"
			let tgt_ref = local_branch.get();
			let local_name = tgt_ref.name().unwrap();
			repo.set_head(local_name)?;
			ret_str = tgt_ref.shorthand().unwrap_or(local_name).to_owned();
		} else if remotes.len() == 1 {
			// Get name of default branch for remote
			let mut remote = repo.find_remote(remotes.first().unwrap())?;
			remote.connect(git2::Direction::Fetch)?;
			let default = remote.default_branch()?;
			// Get the <NAME> in "refs/heads/<NAME>" for remote
			let default_name = default.as_str().unwrap();
			let (_, remote_branch_name) = default_name.rsplit_once('/').unwrap();
			// Check if any local branches are tracking it
			let mut opt_tgt_head: Option<&str> = None;
			for branch in local_branches.iter() {
				let Ok(upstr) = branch.upstream() else {
					continue;
				};
				// Get the <NAME> in "refs/remote/<REMOTE>/<NAME>"
				let upstream_name = upstr.get().name().unwrap();
				let (_, upstream_branch_name) = upstream_name.rsplit_once('/').unwrap();
				// If the branch names match, we have found our branch
				if upstream_branch_name == remote_branch_name {
					opt_tgt_head = Some(branch.get().name().unwrap());
					break;
				}
			}
			let Some(local_name) = opt_tgt_head else {
				return Err(HcError::msg(
					"could not find local branch tracking remote default",
				));
			};
			repo.set_head(local_name)?;
			let head_ref = repo.head()?;
			ret_str = head_ref.shorthand().unwrap_or(local_name).to_owned();
		} else {
			return Err(HcError::msg(
				"repo has multiple local branches and remotes, target is ambiguous",
			));
		}
	}
	repo.checkout_head(Some(make_checkout_builder().force()))?;

	Ok(ret_str)
}

/// Do a `git fetch` for all remotes in the repo.
pub fn fetch(repo_path: &Path) -> HcResult<()> {
	// Open the repo with git2.
	let repo: Repository = Repository::open(repo_path)?;

	// Do a general `git fetch` to get all new refs/tags
	let remotes = repo.remotes()?;
	for remote_name_str in remotes.into_iter().flatten() {
		let mut remote = repo.find_remote(remote_name_str)?;
		let refspecs = remote.fetch_refspecs()?;
		let rs_arr = refspecs.into_iter().flatten().collect::<Vec<&str>>();
		// Fetch the updated remote.
		remote.fetch(rs_arr.as_slice(), Some(&mut make_fetch_opts()), None)?;
	}

	Ok(())
}
