// SPDX-License-Identifier: Apache-2.0

pub mod graphql;
pub mod rest;

use crate::{
	github::{
		graphql::{
			reviews::{GitHubPullRequest, get_reviews},
			user_orgs::{UserOrgData, get_user_orgs},
		},
		rest::code_search::check_fuzzing,
	},
	tls::authenticated_agent::AuthenticatedAgent,
};
use anyhow::{Context, Result};

pub struct GitHub<'a> {
	agent: AuthenticatedAgent<'a>,
}

impl<'a> GitHub<'a> {
	pub fn new(token: &'a str) -> Result<GitHub<'a>> {
		Ok(GitHub {
			agent: AuthenticatedAgent::new(token)?,
		})
	}

	pub fn check_fuzzing(&self, repo_uri: &str) -> Result<bool> {
		check_fuzzing(&self.agent, repo_uri).context("unable to search fuzzing information; please ensure the provided system environment variable exists and contains a valid GitHub API token")
	}

	pub fn get_reviews(&self, owner: &str, repo: &str) -> Result<Vec<GitHubPullRequest>> {
		get_reviews(&self.agent, owner, repo)
	}

	pub fn get_user_orgs(&self, user_login: &str) -> Result<UserOrgData> {
		get_user_orgs(&self.agent, user_login)
	}
}
