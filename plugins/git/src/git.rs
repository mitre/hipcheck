// SPDX-License-Identifier: Apache-2.0

use crate::data::*;

use anyhow::Context;
use anyhow::Result;
use gix::bstr::ByteSlice;
use gix::diff::blob::intern::InternedInput;
use gix::diff::blob::sink::Counter;
use gix::diff::blob::sources::lines_with_terminator;
use gix::diff::blob::Algorithm;
use gix::diff::blob::UnifiedDiffBuilder;
use gix::object;
use gix::objs::tree::EntryKind;
use gix::revision::walk::Sorting;
use gix::revision::Walk;
use gix::traverse::commit::simple::CommitTimeOrder;
use gix::ObjectId;
use gix::Repository;
use jiff::Timestamp;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Mutex;

/// used to cache all of the `RawCommit` from the last repo/HEAD combination analyzed by this
/// plugin
type GitRawCommitCache = Option<(PathBuf, ObjectId, Vec<RawCommit>)>;

/// retrieve a handle to the git repo at this path, as well as determine the commit hash of
/// HEAD
fn initialize_repo<P>(repo_path: P) -> Result<(Repository, ObjectId)>
where
	P: AsRef<Path>,
{
	let repo = gix::discover(repo_path).context("failed to find repo")?;
	let head_commit = repo.head_commit()?.id;
	Ok((repo, head_commit))
}

/// Retrieves an iterator that walks the repo's commits
///
/// Commits are sorted by commit time and the newest commit (HEAD) is seen first
fn get_commit_walker(repo: &Repository, starting_commit: ObjectId) -> Result<Walk<'_>> {
	let repo_walker = repo
		.rev_walk(Some(starting_commit))
		.sorting(Sorting::ByCommitTime(CommitTimeOrder::NewestFirst))
		.all()?;
	Ok(repo_walker)
}

/// Function to call on a commit in the repo to attempt to convert it to type `T`
type MapFn<'a, T> = &'a dyn Fn(&Repository, gix::Commit<'_>) -> Result<T>;
/// Function to call on a commit to determine if iteration should be halted (if Ok(true) is returned)
type BreakNowFn<'a> = &'a dyn Fn(&gix::Commit<'_>) -> Result<bool>;

/// Utility function for walking all of the commits in a git repo and running a function on each commit to generate some result and breaking out of the walk if `break_now` is true
fn walk_commits<'repo, T>(
	repo: &'repo Repository,
	repo_walker: Walk<'repo>,
	transform_fn: MapFn<T>,
	break_now_fn: Option<BreakNowFn>,
) -> Result<Vec<T>> {
	// since we are walking commit by commit, 5,000 was arbitrarily chosen to reduce allocations for small/medium repo sizes
	let mut results = Vec::with_capacity(5_000);
	for object in repo_walker {
		let commit = object?.object()?;
		if let Some(ref break_now_fn) = break_now_fn {
			if let Ok(true) = break_now_fn(&commit) {
				break;
			}
		}
		let res = transform_fn(repo, commit)?;
		results.push(res);
	}
	Ok(results)
}

pub fn get_latest_commit<P>(repo_path: P) -> Result<Option<RawCommit>>
where
	P: AsRef<Path>,
{
	let (repo, head_commit) = initialize_repo(repo_path)?;
	let mut commit_walker = get_commit_walker(&repo, head_commit)?;
	match commit_walker.next() {
		Some(object) => {
			let info = object?;
			let commit = info.object()?;
			let raw_commit = RawCommit::try_from(commit)?;
			Ok(Some(raw_commit))
		}
		None => Ok(None),
	}
}

/// Convert a `gix::Commit` into a `RawCommit`
fn get_raw_commit(_repo: &Repository, commit: gix::Commit) -> Result<RawCommit> {
	let raw_commit = RawCommit::try_from(commit)?;
	Ok(raw_commit)
}

fn get_all_raw_commits_inner<P>(
	repo: &Repository,
	repo_path: P,
	head_commit: ObjectId,
) -> Result<(PathBuf, ObjectId, Vec<RawCommit>)>
where
	P: AsRef<Path>,
{
	let commit_walker = get_commit_walker(repo, head_commit)?;
	let commits = walk_commits(repo, commit_walker, &get_raw_commit, None)?;
	Ok((repo_path.as_ref().to_path_buf(), head_commit, commits))
}

/// Retrieve all of the `RawCommit` from a repo **sorted from newest to oldest**
///
/// This contains a cache of all `RawCommit` to avoid needing to recompute this and should be used as the starting point for a
/// function if all of the desired data can be derived from all of the `RawCommit` in the repo.
///
/// This cache is maintained for a single repo_path/HEAD commit combination. If a new repo/HEAD
/// combination is requested, then the cache will be flushed and updated.
pub fn get_all_raw_commits<P>(repo_path: P) -> Result<Vec<RawCommit>>
where
	P: AsRef<Path>,
{
	// used to cache all of the RawCommits from the last repository analyzed
	static ALL_RAW_COMMITS: Mutex<GitRawCommitCache> = Mutex::new(None);

	let (repo, head_commit) = initialize_repo(repo_path.as_ref())?;
	let mut cache = ALL_RAW_COMMITS.lock().unwrap();

	// if there is a value in cache, and it is the same repo with the same HEAD commit, then we can use the
	// cached value
	if let Some(cached_value) = cache.as_ref() {
		if cached_value.0 == repo_path.as_ref().to_path_buf() && cached_value.1 == head_commit {
			return Ok(cached_value.2.clone());
		}
	}

	// otherwise the cache needs to be updated with the data from this repo_path/HEAD combination
	let updated_value = get_all_raw_commits_inner(&repo, repo_path.as_ref(), head_commit)?;
	let raw_commits = updated_value.2.clone();
	*cache = Some(get_all_raw_commits_inner(&repo, repo_path, head_commit)?);
	Ok(raw_commits)
}

pub fn get_commits_from_date<P>(repo_path: P, cutoff_date: Timestamp) -> Result<Vec<RawCommit>>
where
	P: AsRef<Path>,
{
	let raw_commits = get_all_raw_commits(repo_path)?;
	let raw_commits_since_cutoff: Vec<RawCommit> = raw_commits
		.into_iter()
		.filter(|raw_commit| {
			if let Ok(commit_timestamp) = &raw_commit.committed_on {
				return *commit_timestamp > cutoff_date;
			}
			false
		})
		.collect();
	Ok(raw_commits_since_cutoff)
}

fn diff_objects(old_object: Option<&str>, new_object: Option<&str>) -> Counter<String> {
	let input = InternedInput::new(
		lines_with_terminator(old_object.unwrap_or_default()),
		lines_with_terminator(new_object.unwrap_or_default()),
	);
	gix::diff::blob::diff(
		Algorithm::Myers,
		&input,
		Counter::new(UnifiedDiffBuilder::new(&input)),
	)
}

fn get_diff(repo: &Repository, commit: gix::Commit) -> Result<Diff> {
	let current_tree = commit.tree()?;
	let parent_tree = match commit.parent_ids().next() {
		Some(id) => repo.find_commit(id)?.tree()?,
		// if there is no parent, then this must be the first commit which can be represented with an empty tree
		None => repo.empty_tree(),
	};

	let changes = repo.diff_tree_to_tree(
		Some(&parent_tree),
		Some(&current_tree),
		// this is recommended to increase performance!
		gix::diff::Options::default().with_rewrites(None),
	)?;

	let mut file_diffs = Vec::with_capacity(changes.len());

	for change in changes {
		let change_kind = EntryKind::from(change.entry_mode());
		// check to see if the given change is a file that we can diff
		if !matches!(change_kind, EntryKind::Blob | EntryKind::BlobExecutable) {
			continue;
		}
		match change {
			object::tree::diff::ChangeDetached::Addition {
				location,
				relation: _,
				entry_mode: _,
				id,
			} => {
				let mut file_diff = FileDiff::new(location.to_string());
				let blob = repo.find_object(id)?;
				let new_hunk = String::from_utf8_lossy(&blob.data);

				let diff = diff_objects(None, Some(&new_hunk));
				file_diff.increment_additions(diff.insertions as i64);
				file_diff.set_patch(diff.wrapped);
				file_diffs.push(file_diff);
			}
			object::tree::diff::ChangeDetached::Deletion {
				location,
				relation: _,
				entry_mode: _,
				id,
			} => {
				let mut file_diff = FileDiff::new(location.to_string());
				let object = repo.find_object(id)?;
				let deleted_hunk = String::from_utf8_lossy(&object.data);

				let diff = diff_objects(Some(&deleted_hunk), None);
				file_diff.increment_deletions(diff.removals as i64);
				file_diff.set_patch(diff.wrapped);
				file_diffs.push(file_diff);
			}
			object::tree::diff::ChangeDetached::Modification {
				location,
				previous_entry_mode: _,
				previous_id,
				entry_mode: _,
				id,
			} => {
				let mut file_diff = FileDiff::new(location.to_string());
				let current_blob = repo.find_blob(id)?;
				let previous_blob = repo.find_blob(previous_id)?;
				let current_blob_data = current_blob.data.to_str_lossy();
				let previous_blob_data = previous_blob.data.to_str_lossy();

				let diff = diff_objects(Some(&previous_blob_data), Some(&current_blob_data));
				file_diff.increment_additions(diff.insertions as i64);
				file_diff.increment_deletions(diff.removals as i64);
				file_diff.set_patch(diff.wrapped);
				file_diffs.push(file_diff);
			}
			// because we are not tracking rewrites (for performance reasons), this branch will never get hit
			object::tree::diff::ChangeDetached::Rewrite {
				source_location: _,
				source_entry_mode: _,
				source_relation: _,
				source_id: _,
				diff: _,
				entry_mode: _,
				id: _,
				location: _,
				relation: _,
				copy: _,
			} => {}
		}
	}

	let diff = Diff {
		additions: file_diffs.iter().map(|x| x.additions).sum(),
		deletions: file_diffs.iter().map(|x| x.deletions).sum(),
		file_diffs,
	};
	Ok(diff)
}

pub fn get_diffs<P>(repo_path: P) -> Result<Vec<Diff>>
where
	P: AsRef<Path>,
{
	let (repo, head_commit) = initialize_repo(repo_path)?;
	let commit_walker = get_commit_walker(&repo, head_commit)?;
	walk_commits(&repo, commit_walker, &get_diff, None)
}

/// Get all of the contributors (committers and authors) in a repo's history
pub fn get_contributors<P>(repo_path: P) -> Result<Vec<Contributor>>
where
	P: AsRef<Path>,
{
	let commits = get_all_raw_commits(repo_path)?;
	let mut contributors: Vec<Contributor> = commits
		.into_iter()
		.map(|raw_commit| [raw_commit.author, raw_commit.committer])
		.flat_map(|contributors| contributors.into_iter())
		.collect();
	contributors.sort();
	contributors.dedup();
	Ok(contributors)
}

/// Get the `CommitDiff` for a commit
fn get_commit_diff(repo: &Repository, commit: gix::Commit) -> Result<CommitDiff> {
	let raw_commit = get_raw_commit(repo, commit.clone())?;
	let diff = get_diff(repo, commit)?;
	Ok(CommitDiff::new(raw_commit.into(), diff))
}

pub fn get_commit_diffs<P>(repo_path: P) -> Result<Vec<CommitDiff>>
where
	P: AsRef<Path>,
{
	let (repo, head_commit) = initialize_repo(repo_path)?;
	let commit_walker = get_commit_walker(&repo, head_commit)?;
	let commit_diffs = walk_commits(&repo, commit_walker, &get_commit_diff, None)?;
	Ok(commit_diffs)
}
