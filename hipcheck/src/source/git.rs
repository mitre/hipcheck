// SPDX-License-Identifier: Apache-2.0

//! Git related types and implementations for pulling/cloning source repos.

use crate::{
	error::{Error as HcError, Result as HcResult},
	hc_error,
};
use gix::{bstr::ByteSlice, refs::FullName, remote, ObjectId};
use std::path::Path;
use url::Url;

/// default options to use when fetching a repo with `gix`
fn fetch_options(url: &Url, dest: &Path) -> gix::clone::PrepareFetch {
	gix::clone::PrepareFetch::new(
		url.as_str(),
		dest,
		gix::create::Kind::WithWorktree,
		gix::create::Options::default(),
		gix::open::Options::default(),
	)
	.expect("fetch options must be valid to perform a clone")
}

/// fast-forward HEAD of current repo to a new_commit
///
/// returns new ObjectId (hash) of updated HEAD upon success
fn fast_forward_to_hash(
	repo: &gix::Repository,
	current_head: gix::Head,
	new_object_id: gix::ObjectId,
) -> HcResult<ObjectId> {
	let current_id = current_head
		.id()
		.ok_or_else(|| hc_error!("Could not determine hash of current HEAD"))?;

	if current_id == new_object_id {
		log::debug!("skipping fast-forward, IDs match");
		return Ok(current_id.into());
	}
	let edit = gix::refs::transaction::RefEdit {
		change: gix::refs::transaction::Change::Update {
			log: gix::refs::transaction::LogChange {
				mode: gix::refs::transaction::RefLog::AndReference,
				force_create_reflog: false,
				message: format!("fast-forward HEAD from {} to {}", current_id, new_object_id)
					.into(),
			},
			expected: gix::refs::transaction::PreviousValue::Any,
			new: gix::refs::Target::Object(new_object_id),
		},
		name: FullName::try_from("HEAD").unwrap(),
		deref: true,
	};
	log::trace!(
		"attempting fast-forward from {} to {}",
		current_id,
		new_object_id
	);

	// commit change to the repo and the reflog
	repo.refs
		.transaction()
		.prepare(
			[edit],
			gix::lock::acquire::Fail::Immediately,
			gix::lock::acquire::Fail::Immediately,
		)?
		.commit(Some(Default::default()))?;
	log::trace!("fast-forward successful");
	Ok(new_object_id)
}

/// Clone a repo from the given url to a destination path in the filesystem.
pub fn clone(url: &Url, dest: &Path) -> HcResult<()> {
	log::debug!("attempting to clone {} to {:?}", url.as_str(), dest);
	std::fs::create_dir_all(dest)?;
	let mut fetch_options = fetch_options(url, dest);
	let (mut checkout, _) = fetch_options
		.fetch_then_checkout(gix::progress::Discard, &gix::interrupt::IS_INTERRUPTED)?;
	let _ = checkout.main_worktree(gix::progress::Discard, &gix::interrupt::IS_INTERRUPTED)?;
	log::info!("Successfully cloned {} to {:?}", url.as_str(), dest);
	Ok(())
}

/// For a given repo, checkout a particular ref in a detached HEAD state.
///
/// 1. If a refspec is passed, then attempt to fast-forward to the specified revision
/// 2. If no ref is provided, then attempt to fast-forward repo to HEAD of the default remote.
/// 3. If there is no default remote, then attempt to set HEAD to match upstream of local branch
///
/// If none of these are possible, then error due to inability to infer target
pub fn checkout(repo_path: &Path, refspec: Option<String>) -> HcResult<gix::ObjectId> {
	let repo = gix::open(repo_path)?;
	let head = repo.head()?;

	// if a refspec was given attempt to resolve it, error if unable to resolve
	if let Some(refspec) = refspec {
		log::trace!("attempting to find refspec [{}]", refspec);
		// try refspec as given
		let target = match repo.rev_parse(refspec.as_str()) {
			Ok(rev) => {
				let oid = rev
					.single()
					.ok_or_else(|| hc_error!("ref '{}' was not a unique identifier", refspec))?;
				repo.find_object(oid)?.id
			}
			Err(e) => {
				return Err(hc_error!(
					"Could not find repo with provided refspec: {}",
					e
				));
			}
		};
		log::trace!("found refspec: {:?}", target);
		return fast_forward_to_hash(&repo, head, target);
	}

	// Determine if there is a default remote, if there is determine what it thinks HEAD is and
	// fast-forward to the remote HEAD
	if let Some(Ok(default_remote)) = repo.find_default_remote(remote::Direction::Fetch) {
		if let Some(remote_name) = default_remote.name() {
			if let Ok(mut remote_head) =
				repo.find_reference(format!("refs/remotes/{}/HEAD", remote_name.as_bstr()).as_str())
			{
				let target = remote_head.peel_to_id_in_place()?;
				return fast_forward_to_hash(&repo, head, target.into());
			}
		}
	}

	let mut local_branches = repo.branch_names();
	if local_branches.len() == 1 {
		let mut local_branch = repo.find_reference(
			format!("refs/heads/{}", local_branches.pop_first().unwrap()).as_str(),
		)?;
		let tip_of_local_branch = local_branch.peel_to_id_in_place()?;
		return fast_forward_to_hash(&repo, head, tip_of_local_branch.into());
	}
	Err(HcError::msg("target is ambiguous"))
}

/// TODO: redo commit history to add support for fetch/clone/checkout separately
/// TODO: add support for visual progress indicators
/// Perform a `git fetch` for all remotes in the repo.
pub fn fetch(repo_path: &Path) -> HcResult<()> {
	log::debug!("Fetching: {:?}", repo_path);
	let repo = gix::open(repo_path)?;
	let remote_names = repo.remote_names();
	for remote_name in remote_names {
		log::trace!("Attempt to fetch remote: {}", remote_name.as_bstr());
		let remote = repo.find_remote(remote_name.as_bstr())?;
		remote
			.connect(gix::remote::Direction::Fetch)?
			.prepare_fetch(gix::progress::Discard, Default::default())?
			.receive(gix::progress::Discard, &gix::interrupt::IS_INTERRUPTED)?;
	}
	Ok(())
}
