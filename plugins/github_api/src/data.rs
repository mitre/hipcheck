// SPDX-License-Identifier: Apache-2.0

use crate::{
	code_search::search_code_request, graphql::get_all_reviews, types::GitHubPullRequest,
	util::authenticated_agent::AuthenticatedAgent,
};
use anyhow::{Context, Result};
use std::rc::Rc;

pub struct GitHub<'a> {
	owner: &'a str,
	repo: &'a str,
	agent: AuthenticatedAgent<'a>,
}

impl<'a> GitHub<'a> {
	pub fn new(owner: &'a str, repo: &'a str, token: &'a str) -> Result<GitHub<'a>> {
		Ok(GitHub {
			owner,
			repo,
			agent: AuthenticatedAgent::new(token),
		})
	}

	pub fn fuzz_check(&self, repo_uri: Rc<String>) -> Result<bool> {
		search_code_request(&self.agent, repo_uri).context("unable to search fuzzing information; please check the HC_GITHUB_TOKEN system environment variable")
	}

	pub fn get_reviews_for_pr(&self) -> Result<Vec<GitHubPullRequest>> {
		get_all_reviews(&self.agent, self.owner, self.repo)
	}
}
