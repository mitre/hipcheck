// SPDX-License-Identifier: Apache-2.0

use crate::data::*;
use anyhow::{Context, Result};
use git2::DiffFormat;
use git2::DiffLineType;
use git2::Repository;
use git2::Revwalk;
use git2::Sort;
use jiff::Timestamp;
use std::str::FromStr;
use std::{convert::AsRef, path::Path};

fn get_repo_head<'a>(repo: &'a Repository) -> Result<Revwalk<'a>> {
	let mut revwalk = repo.revwalk().context("Unable to determine HEAD")?;
	// as of right now, we always want the commits sorted from newest to oldest
	revwalk
		.set_sorting(Sort::TIME)
		.context("Unable to set commit sorting")?;
	revwalk.push_head().context("Unable to push repo HEAD")?;
	Ok(revwalk)
}

/// Function to call on every commit in the repo to accumulate some type <T> for each commit in the repo
type MapFn<'a, T> = &'a dyn Fn(&Repository, git2::Commit<'_>) -> Result<T>;
/// Function that will break out of the git tree walking process, if this returns Ok(true)
type BreakNowFn<'a> = &'a dyn Fn(&git2::Commit<'_>) -> bool;

/// Utility function for walking all of the commits in a git repo and running a function on each commit to generate some result and breaking out of the walk if `break_now` is true
fn walk_commits<P, T>(repo: P, func: MapFn<T>, break_now: Option<BreakNowFn>) -> Result<Vec<T>>
where
	P: AsRef<Path>,
{
	let repo = Repository::open(repo).context("Could not open repository")?;
	let revwalk = get_repo_head(&repo)?;
	// since we are walking commit by commit, 5,000 was arbitrarily chosen to reduce allocations for small/medium repo sizes
	let mut results = Vec::with_capacity(5_000);
	for oid in revwalk {
		let oid = oid?;
		let commit = repo.find_commit(oid)?;
		if let Some(ref break_now) = break_now {
			if break_now(&commit) {
				break;
			}
		}
		let res = func(&repo, commit)?;
		results.push(res);
	}
	Ok(results)
}

fn get_raw_commit(_repo: &Repository, commit: git2::Commit) -> Result<RawCommit> {
	Ok(RawCommit::from(commit))
}

/// retrieve all of the raw commits in a repos history
pub fn get_raw_commits<P: AsRef<Path>>(repo: P) -> Result<Vec<RawCommit>> {
	walk_commits(repo, &get_raw_commit, None)
}

fn get_commit(_repo: &Repository, commit: git2::Commit) -> Result<Commit> {
	Ok(Commit::from(commit))
}

/// retrieve all of the commits in a repos history
pub fn get_commits<P: AsRef<Path>>(repo: P) -> Result<Vec<Commit>> {
	walk_commits(repo, &get_commit, None)
}

/// retrieve all of the commits in a repos history that were committed after a specific time
pub fn get_commits_from_date<P>(repo: P, since: Timestamp) -> Result<Vec<Commit>>
where
	P: AsRef<Path>,
{
	walk_commits(
		repo,
		&get_commit,
		Some(&|commit| {
			let raw_commit = RawCommit::from(commit.clone());
			if let Ok(commit_timestamp) = raw_commit.committed_on {
				if let Ok(commit_timestamp) = jiff::Timestamp::from_str(&commit_timestamp) {
					return commit_timestamp < since;
				}
			}
			false
		}),
	)
}

/// from a given commit in a repo, attempt to generate the Diff between this commit and its parent
fn get_diff_raw(repo: &Repository, commit: git2::Commit) -> Result<Diff> {
	let current_tree = commit
		.tree()
		.context("Could not determine tree for the current commit")?;

	// if there is no previous commit, then this must be the first commit
	let previous_tree = match commit.parents().next() {
		Some(previous_commit) => Some(
			previous_commit
				.tree()
				.context("Could not determine tree for the previous commit")?,
		),
		None => None,
	};

	let diff = repo
		.diff_tree_to_tree(previous_tree.as_ref(), Some(&current_tree), None)
		.context("Could not diff current commit to previous commit")?;

	let stats = diff.stats().context("Could not determine stats for diff")?;

	let total_insertions_in_commit = stats.insertions();
	let total_deletions_in_commit = stats.deletions();

	// arbitrary pre-allocation to hold FileDiffs for this commit to reduce number of needed allocations
	let mut file_diffs: Vec<FileDiff> = Vec::with_capacity(128);
	// iterate over all of the patches in this commit to generate all of the FileDiffs for this commit
	diff.print(DiffFormat::Patch, |delta, _hunk, line| {
		if let Some(file_name) = delta.new_file().path() {
			let file_name = file_name.to_string_lossy();

			let file_diff: &mut FileDiff = match file_diffs
				.iter_mut()
				.find(|fd| fd.file_name.as_str() == file_name)
			{
				Some(file_diff) => file_diff,
				None => {
					file_diffs.push(FileDiff {
						file_name: file_name.to_string().into(),
						additions: 0,
						deletions: 0,
						patch: String::new(),
					});
					// unwrap is safe because we just pushed
					file_diffs.last_mut().unwrap()
				}
			};

			// add the line to the patch
			file_diff.add_to_patch(line.content());

			match line.origin_value() {
				DiffLineType::Addition => file_diff.increment_additions(1),
				DiffLineType::Deletion => file_diff.increment_deletions(1),
				_ => {}
			}
		}
		true
	})
	.context("Could not generate FileDiff for commit")?;

	Ok(Diff {
		additions: Some(total_insertions_in_commit as i64),
		deletions: Some(total_deletions_in_commit as i64),
		file_diffs,
	})
}

/// get the diff between each commit and its parent in the repo
pub fn get_diffs<P: AsRef<Path>>(repo: P) -> Result<Vec<Diff>> {
	walk_commits(repo, &get_diff_raw, None)
}

fn get_commit_diff(repo: &Repository, commit: git2::Commit) -> Result<CommitDiff> {
	let hc_commit = Commit::from(commit.clone());
	let diff = get_diff_raw(repo, commit)?;
	Ok(CommitDiff::new(hc_commit, diff))
}

/// gets all of the commits and diffs in the repo
pub fn get_commit_diffs<P: AsRef<Path>>(repo: P) -> Result<Vec<CommitDiff>> {
	walk_commits(repo, &get_commit_diff, None)
}
