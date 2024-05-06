// SPDX-License-Identifier: Apache-2.0

use crate::git::{Diff, RawCommit};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct GitHubFullPullRequest {
	pub pull_request: GitHubPullRequest,
	pub commits: Vec<RawCommit>,
	pub diffs: Vec<Diff>,
}

#[derive(Debug, Deserialize)]
pub struct GitHubPullRequestWithCommits {
	pub pull_request: GitHubPullRequest,
	pub commits: Vec<RawCommit>,
}

#[derive(Debug, Deserialize)]
pub struct GitHubPullRequest {
	pub number: u64,
	pub reviews: u64,
}
