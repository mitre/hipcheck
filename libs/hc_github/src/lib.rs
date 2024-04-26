// SPDX-License-Identifier: Apache-2.0

pub mod code_search;
mod data;
mod graphql;
mod graphql_pr;

use std::{io, rc::Rc};

use crate::code_search::search_code_request;
use crate::data::*;
use crate::graphql::get_all_reviews;
use crate::graphql_pr::get_all_pr_reviews;
use hc_common::context::Context as _;
use hc_common::{error::Result, log};
use hc_git::parse::github_diff;
use ureq::Agent;

pub struct GitHub<'a> {
	owner: &'a str,
	repo: &'a str,
	agent: Agent,
}

impl<'a> GitHub<'a> {
	pub fn new(owner: &'a str, repo: &'a str, token: &'a str) -> GitHub<'a> {
		let agent = Agent::new().auth_kind("token", token).build();

		GitHub { owner, repo, agent }
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
	agent: Agent,
}

impl<'a> GitHubPr<'a> {
	pub fn new(
		owner: &'a str,
		repo: &'a str,
		pull_request: &'a u64,
		token: &'a str,
	) -> GitHubPr<'a> {
		let agent = Agent::new().auth_kind("token", token).build();

		GitHubPr {
			owner,
			repo,
			pull_request,
			agent,
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

	fn get_diffs_for_single_pr(&self) -> io::Result<String> {
		let url = &format!(
			"https://patch-diff.githubusercontent.com/raw/{}/{}/pull/{}.diff",
			self.owner, self.repo, self.pull_request
		);
		log::trace!("diff url is  {:#?}", url);
		self.agent.get(url).call().into_string()
	}
}
