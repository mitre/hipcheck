// SPDX-License-Identifier: Apache-2.0

//! Functions and types for data retrieval.

pub mod git;
pub mod git_command;
pub mod npm;

mod code_quality;
mod es_lint;
mod github;
mod hash;
mod query;

pub use query::*;

use crate::error::{Context, Error, Result};
use git::{Commit, CommitContributor, Contributor, Diff};
use github::*;
use pathbuf::pathbuf;
use serde::Serialize;
use std::{path::Path, sync::Arc};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependencies {
	pub language: Lang,
	pub deps: Vec<Arc<String>>,
}

impl Dependencies {
	pub fn resolve(repo: &Path, version: String) -> Result<Dependencies> {
		match Lang::detect(repo) {
			language @ Lang::JavaScript => {
				let deps = npm::get_dependencies(repo, version)?
					.into_iter()
					.map(Arc::new)
					.collect();
				Ok(Dependencies { language, deps })
			}
			Lang::Unknown => Err(Error::msg(
				"can't identify a known language in the repository",
			)),
		}
	}
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum Lang {
	JavaScript,
	Unknown,
}

impl Lang {
	fn detect(repo: &Path) -> Lang {
		if pathbuf![repo, "package.json"].exists() {
			Lang::JavaScript
		} else {
			Lang::Unknown
		}
	}
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Fuzz {
	pub exists: bool,
}

pub fn get_fuzz_check(token: &str, repo_uri: Arc<String>) -> Result<Fuzz> {
	let github = GitHub::new("google", "oss-fuzz", token)?;

	let github_result = github
		.fuzz_check(repo_uri)
		.context("unable to query fuzzing info")?;

	let result = Fuzz {
		exists: github_result,
	};

	Ok(result)
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct PullRequest {
	pub id: u64,
	pub reviews: u64,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct SinglePullRequest {
	pub id: u64,
	pub reviews: u64,
	pub commits: Vec<Arc<Commit>>,
	pub contributors: Vec<Arc<Contributor>>,
	pub commit_contributors: Vec<CommitContributor>,
	pub diffs: Vec<Arc<Diff>>,
}

pub fn get_pull_request_reviews_from_github(
	owner: &str,
	repo: &str,
	token: &str,
) -> Result<Vec<PullRequest>> {
	let github = GitHub::new(owner, repo, token)?;

	let results = github
		.get_reviews_for_pr()
		.context("failed to get pull request reviews from the GitHub API, please check the HC_GITHUB_TOKEN system environment variable")?
		.into_iter()
		.map(|pr| PullRequest {
			id: pr.number,
			reviews: pr.reviews,
		})
		.collect();

	Ok(results)
}

pub fn get_single_pull_request_review_from_github(
	owner: &str,
	repo: &str,
	pull_request: &u64,
	token: &str,
) -> Result<SinglePullRequest> {
	let github_pr = GitHubPr::new(owner, repo, pull_request, token)?;

	let github_result = github_pr
		.get_review_for_single_pr()
		.context("failed to get pull request review from the GitHub API")?;

	log::trace!("full pull request is {:#?}", github_result);

	let commits = github_result
		.commits
		.iter()
		.map(|raw| {
			Arc::new(Commit {
				hash: raw.hash.to_owned(),
				written_on: raw.written_on.to_owned(),
				committed_on: raw.committed_on.to_owned(),
			})
		})
		.collect();

	let mut contributors: Vec<Arc<Contributor>> = github_result
		.commits
		.iter()
		.flat_map(|raw| {
			[
				Arc::new(raw.author.to_owned()),
				Arc::new(raw.committer.to_owned()),
			]
		})
		.collect();

	contributors.sort();
	contributors.dedup();

	let commit_contributors = github_result
		.commits
		.iter()
		.enumerate()
		.map(|(commit_id, raw)| {
			// SAFETY: These `position` calls are guaranteed to return `Some`
			// given how `contributors` is constructed from `get_review_for_single_pr()`
			let author_id = contributors
				.iter()
				.position(|c| c.as_ref() == &raw.author)
				.unwrap();
			let committer_id = contributors
				.iter()
				.position(|c| c.as_ref() == &raw.author)
				.unwrap();

			CommitContributor {
				commit_id,
				author_id,
				committer_id,
			}
		})
		.collect();

	let diffs = github_result
		.diffs
		.iter()
		.map(|diff| Arc::new(diff.to_owned()))
		.collect();

	let result = SinglePullRequest {
		id: github_result.pull_request.number,
		reviews: github_result.pull_request.reviews,
		commits,
		contributors,
		commit_contributors,
		diffs,
	};

	Ok(result)
}
