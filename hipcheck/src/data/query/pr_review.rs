// SPDX-License-Identifier: Apache-2.0

//! Query group for GitHub pull request reviews.

use super::github::GitHubProvider;
use crate::config::ConfigSource;
use crate::context::Context;
use crate::data::get_pull_request_reviews_from_github;
use crate::data::get_single_pull_request_review_from_github;
use crate::data::git::Commit;
use crate::data::git::CommitContributorView;
use crate::data::git::CommitDiff;
use crate::data::git::Contributor;
use crate::data::git::ContributorView;
use crate::data::PullRequest;
use crate::data::SinglePullRequest;
use crate::error::Error;
use crate::error::Result;
use std::sync::Arc;

/// A query that provides GitHub pull request reviews
#[salsa::query_group(PullRequestReviewProviderStorage)]
pub trait PullRequestReviewProvider: ConfigSource + GitHubProvider {
	/// Returns the available pull request reviews
	fn pull_request_reviews(&self) -> Result<Arc<Vec<Arc<PullRequest>>>>;

	/// Returns the available single pull request review
	fn single_pull_request_review(&self) -> Result<Arc<SinglePullRequest>>;

	/// Returns the commits associated with a contributor for a single pull request
	fn get_pr_commits_for_contributor(
		&self,
		contributor: Arc<Contributor>,
	) -> Result<ContributorView>;

	/// Returns the contributors associed with a commit for a single pull request
	fn get_pr_contributors_for_commit(&self, commit: Arc<Commit>) -> Result<CommitContributorView>;

	/// Returns all commit-diff pairs
	fn get_pr_commit_diffs(&self) -> Result<Arc<Vec<CommitDiff>>>;
}

/// Derived query implementations.  The returned `PullRequest` values
/// are wrapped in an `Rc` to keep cloning cheap and let other types
/// hold references to them.

fn pull_request_reviews(db: &dyn PullRequestReviewProvider) -> Result<Arc<Vec<Arc<PullRequest>>>> {
	let token = db.github_api_token().ok_or_else(|| {
		Error::msg("missing GitHub token in environment variable HC_GITHUB_TOKEN. Please set this system variable.")
	})?;
	let prs = get_pull_request_reviews_from_github(&db.owner()?, &db.repo()?, &token)?
		.into_iter()
		.map(Arc::new)
		.collect();
	Ok(Arc::new(prs))
}

/// Query implementation.  The returned `SinglePullRequest` value is
/// wrapped in an `Rc` to keep cloning cheap and let other types hold
/// references to them.

fn single_pull_request_review(
	db: &dyn PullRequestReviewProvider,
) -> Result<Arc<SinglePullRequest>> {
	let token =
		db.github_api_token().ok_or_else(|| {
			Error::msg("missing GitHub token in environment variable HC_GITHUB_TOKEN. Please set this system variable.")
		})?;
	let pr = get_single_pull_request_review_from_github(
		&db.owner()?,
		&db.repo()?,
		&db.pull_request()?,
		&token,
	)?;
	Ok(Arc::new(pr))
}

fn get_pr_commits_for_contributor(
	db: &dyn PullRequestReviewProvider,
	contributor: Arc<Contributor>,
) -> Result<ContributorView> {
	// Get the pull request
	let pr = db
		.single_pull_request_review()
		.context("failed to get pull request")?;

	// Get the index of the contributor
	let contributor_id = pr
		.contributors
		.iter()
		.position(|c| c == &contributor)
		.ok_or_else(|| Error::msg("failed to find contributor"))?;

	// Find commits that have that contributor
	let commits = pr
		.commit_contributors
		.iter()
		.filter_map(|com_con| {
			if com_con.author_id == contributor_id || com_con.committer_id == contributor_id {
				// SAFETY: This index is guaranteed to be valid in
				// `pr.commits` because of how it and `commit_contributors`
				// are constructed from `get_single_pull_request_review_from_github`
				Some(Arc::clone(&pr.commits[com_con.commit_id]))
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

fn get_pr_contributors_for_commit(
	db: &dyn PullRequestReviewProvider,
	commit: Arc<Commit>,
) -> Result<CommitContributorView> {
	// Get the pull request
	let pr = db
		.single_pull_request_review()
		.context("failed to get pull request")?;

	// Get the index of the commit
	let commit_id = pr
		.commits
		.iter()
		.position(|c| c.hash == commit.hash)
		.ok_or_else(|| Error::msg("failed to find commit"))?;

	// Find the author and committer for that commit
	pr.commit_contributors
		.iter()
		.find(|com_con| com_con.commit_id == commit_id)
		.map(|com_con| {
			// SAFETY: These indices are guaranteed to be valid in
			// `pr.contributors` because of how `commit_contributors` is
			// constructed from `get_single_pull_request_review_from_github1.
			let author = Arc::clone(&pr.contributors[com_con.author_id]);
			let committer = Arc::clone(&pr.contributors[com_con.committer_id]);

			CommitContributorView {
				commit,
				author,
				committer,
			}
		})
		.ok_or_else(|| Error::msg("failed to find contributor info"))
}

fn get_pr_commit_diffs(db: &dyn PullRequestReviewProvider) -> Result<Arc<Vec<CommitDiff>>> {
	let pr = db
		.single_pull_request_review()
		.context("failed to get pull request")?;

	let commit_diffs = Iterator::zip(pr.commits.iter(), pr.diffs.iter())
		.map(|(commit, diff)| CommitDiff {
			commit: Arc::clone(commit),
			diff: Arc::clone(diff),
		})
		.collect();

	Ok(Arc::new(commit_diffs))
}
