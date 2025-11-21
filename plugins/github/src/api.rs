// SPDX-License-Identifier: Apache-2.0

use crate::{
	graphql::reviews::get_all_reviews, rest::code_search::search_code_request,
	tls::authenticated_agent::AuthenticatedAgent, types::GitHubPullRequest,
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
			agent: AuthenticatedAgent::new(token)?,
		})
	}

	pub fn fuzz_check(&self, repo_uri: Rc<String>) -> Result<bool> {
		search_code_request(&self.agent, repo_uri).context("unable to search fuzzing information; please ensure the provided system environment variable exists and contains a valid GitHub API token")
	}

	pub fn get_reviews_for_pr(&self) -> Result<Vec<GitHubPullRequest>> {
		get_all_reviews(&self.agent, self.owner, self.repo)
	}
}
