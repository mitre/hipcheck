// SPDX-License-Identifier: Apache-2.0

mod authenticated_agent;
pub mod code_search;
mod data;
mod graphql;
mod graphql_pr;
mod hidden;

use crate::code_search::search_code_request;
use crate::git::parse::github_diff;
use crate::github::authenticated_agent::AuthenticatedAgent;
use crate::github::data::*;
use crate::github::graphql::get_all_reviews;
use crate::github::graphql_pr::get_all_pr_reviews;
use hc_common::context::Context as _;
use hc_common::{error::Result, log};
use std::rc::Rc;

pub struct GitHub<'a> {
	owner: &'a str,
	repo: &'a str,
	agent: AuthenticatedAgent<'a>,
}

impl<'a> GitHub<'a> {
	pub fn new(owner: &'a str, repo: &'a str, token: &'a str) -> GitHub<'a> {
		GitHub {
			owner,
			repo,
			agent: AuthenticatedAgent::new(token),
		}
	}

	pub fn fuzz_check(&self, repo_uri: Rc<String>) -> Result<bool> {
		search_code_request(&self.agent, repo_uri).context("unable to search fuzzing information; please check the HC_GITHUB_TOKEN system environment variable")
	}

	pub fn get_reviews_for_pr(&self) -> Result<Vec<GitHubPullRequest>> {
		get_all_reviews(&self.agent, self.owner, self.repo)
	}
}

pub struct GitHubPr<'a> {
	owner: &'a str,
	repo: &'a str,
	pull_request: &'a u64,
	agent: AuthenticatedAgent<'a>,
}

impl<'a> GitHubPr<'a> {
	pub fn new(
		owner: &'a str,
		repo: &'a str,
		pull_request: &'a u64,
		token: &'a str,
	) -> GitHubPr<'a> {
		GitHubPr {
			owner,
			repo,
			pull_request,
			agent: AuthenticatedAgent::new(token),
		}
	}

	pub fn get_review_for_single_pr(&self) -> Result<GitHubFullPullRequest> {
		let number = *self.pull_request as i64;
		let reviews = get_all_pr_reviews(&self.agent, self.owner, self.repo, &number)?;
		let raw_diffs = self.get_diffs_for_single_pr()?;
		log::trace!("raw diffs are {:#?}", raw_diffs);
		let diffs = github_diff(&raw_diffs)?;
		log::trace!("diffs are {:#?}", diffs);

		let review = GitHubFullPullRequest {
			pull_request: reviews.pull_request,
			commits: reviews.commits,
			diffs,
		};

		Ok(review)
	}

	fn get_diffs_for_single_pr(&self) -> Result<String> {
		let url = &format!(
			"https://patch-diff.githubusercontent.com/raw/{}/{}/pull/{}.diff",
			self.owner, self.repo, self.pull_request
		);
		log::trace!("diff url is  {:#?}", url);
		Ok(self.agent.get(url).call()?.into_string()?)
	}
}
