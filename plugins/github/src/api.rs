// SPDX-License-Identifier: Apache-2.0

use crate::{
	graphql::{
		reviews::get_all_reviews,
		user_orgs::{UserOrgData, get_user_orgs},
	},
	rest::code_search::detect_oss_fuzz_participation,
	tls::authenticated_agent::AuthenticatedAgent,
	types::GitHubPullRequest,
};
use anyhow::{Context, Result};
use std::rc::Rc;

pub struct GitHub<'a> {
	agent: AuthenticatedAgent<'a>,
}

impl<'a> GitHub<'a> {
	pub fn new(token: &'a str) -> Result<GitHub<'a>> {
		Ok(GitHub {
			agent: AuthenticatedAgent::new(token)?,
		})
	}

	pub fn fuzz_check(&self, repo_uri: Rc<String>) -> Result<bool> {
		detect_oss_fuzz_participation(&self.agent, repo_uri).context("unable to search fuzzing information; please ensure the provided system environment variable exists and contains a valid GitHub API token")
	}

	pub fn get_reviews_for_pr(&self, owner: &str, repo: &str) -> Result<Vec<GitHubPullRequest>> {
		get_all_reviews(&self.agent, owner, repo)
	}

	pub fn get_user_orgs(&self, user_login: &str) -> Result<UserOrgData> {
		get_user_orgs(&self.agent, user_login)
	}
}
