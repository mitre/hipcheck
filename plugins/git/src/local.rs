// SPDX-License-Identifier: Apache-2.0

//! Copies of functions from main.rs that do not run as queries
//! This is a temporary solution until batching is implemented

use crate::{
	data::{Commit, CommitContributor, CommitContributorView, Contributor, ContributorView},
	util::git_command::get_commits,
};
use hipcheck_sdk::{prelude::*, types::LocalGitRepo};

/// Returns all commits extracted from the repository
pub fn local_commits(repo: LocalGitRepo) -> Result<Vec<Commit>> {
	let path = &repo.path;
	let raw_commits = get_commits(path).map_err(|e| {
		log::error!("failed to get raw commits: {}", e);
		Error::UnspecifiedQueryState
	})?;
	let commits = raw_commits
		.iter()
		.map(|raw| Commit {
			hash: raw.hash.to_owned(),
			written_on: raw.written_on.to_owned(),
			committed_on: raw.committed_on.to_owned(),
		})
		.collect();

	Ok(commits)
}

/// Returns all contributors to the repository
pub fn local_contributors(repo: LocalGitRepo) -> Result<Vec<Contributor>> {
	let path = &repo.path;
	let raw_commits = get_commits(path).map_err(|e| {
		log::error!("failed to get raw commits: {}", e);
		Error::UnspecifiedQueryState
	})?;

	let mut contributors: Vec<_> = raw_commits
		.iter()
		.flat_map(|raw| [raw.author.to_owned(), raw.committer.to_owned()])
		.collect();

	contributors.sort();
	contributors.dedup();

	Ok(contributors)
}

/// Returns the commits associated with a given contributor (identified by e-mail address in the `details` value)
pub fn local_commits_for_contributor(
	all_commits: &[Commit],
	contributors: &[Contributor],
	commit_contributors: &[CommitContributor],
	email: &str,
) -> Result<ContributorView> {
	// Get the index of the contributor
	let contributor_id = contributors
		.iter()
		.position(|c| c.email == email)
		.ok_or_else(|| {
			log::error!("failed to find contributor");
			Error::UnspecifiedQueryState
		})?;

	// Get the contributor
	let contributor = contributors[contributor_id].clone();

	// Find commits that have that contributor
	let commits = commit_contributors
		.iter()
		.filter_map(|com_con| {
			if com_con.author_id == contributor_id || com_con.committer_id == contributor_id {
				// SAFETY: This index is guaranteed to be valid in
				// `all_commits` because of how it and `commit_contributors`
				// are constructed from `db.raw_commits()`
				Some(all_commits[com_con.commit_id].clone())
			} else {
				None
			}
		})
		.collect();

	Ok(ContributorView {
		contributor,
		commits,
	})
}

/// Returns the contributor view for a given commit (idenftied by hash in the `details` field)
pub fn local_contributors_for_commit(
	commits: &[Commit],
	contributors: &[Contributor],
	commit_contributors: &[CommitContributor],
	hash: &str,
) -> Result<CommitContributorView> {
	// Get the index of the commit
	let commit_id = commits.iter().position(|c| c.hash == hash).ok_or_else(|| {
		log::error!("failed to find contributor");
		Error::UnspecifiedQueryState
	})?;

	// Get the commit
	let commit = commits[commit_id].clone();

	// Find the author and committer for that commit
	commit_contributors
		.iter()
		.find(|com_con| com_con.commit_id == commit_id)
		.map(|com_con| {
			// SAFETY: These indices are guaranteed to be valid in
			// `contributors` because of how `commit_contributors` is
			// constructed from it.
			let author = contributors[com_con.author_id].clone();
			let committer = contributors[com_con.committer_id].clone();

			CommitContributorView {
				commit,
				author,
				committer,
			}
		})
		.ok_or_else(|| {
			log::error!("failed to find contributor info");
			Error::UnspecifiedQueryState
		})
}

pub fn local_commit_contributors(
	repo: LocalGitRepo,
	contributors: &[Contributor],
) -> Result<Vec<CommitContributor>> {
	let path = &repo.path;
	let raw_commits = get_commits(path).map_err(|e| {
		log::error!("failed to get raw commits: {}", e);
		Error::UnspecifiedQueryState
	})?;

	let commit_contributors = raw_commits
		.iter()
		.enumerate()
		.map(|(commit_id, raw)| {
			// SAFETY: These `position` calls are guaranteed to return `Some`
			// given how `contributors` is constructed from `db.raw_commits()`
			let author_id = contributors.iter().position(|c| c == &raw.author).unwrap();
			let committer_id = contributors
				.iter()
				.position(|c| c == &raw.committer)
				.unwrap();

			CommitContributor {
				commit_id,
				author_id,
				committer_id,
			}
		})
		.collect();

	Ok(commit_contributors)
}
