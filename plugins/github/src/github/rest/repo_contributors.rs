// SPDX-License-Identifier: Apache-2.0

use crate::tls::authenticated_agent::AuthenticatedAgent;
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Check if the given repo participates in OSS-Fuzz.
pub fn get_repo_contributors(
	agent: &AuthenticatedAgent<'_>,
	owner: &str,
	repo: &str,
) -> Result<Vec<Contributor>> {
	let endpoint = endpoint(owner, repo);

	let contributors = agent
		.get(&endpoint)
		.call()?
		.into_json::<Value>()
		.map_err(|_| {
			anyhow!(
				"unable to get repository contributors for GitHub repo '{}/{}'",
				owner,
				repo
			)
		})?;

	let contributors = serde_json::from_value(contributors)?;

	Ok(contributors)
}

fn endpoint(owner: &str, repo: &str) -> String {
	format!(
		"https://api.github.com/repos/{}/{}/contributors",
		owner, repo
	)
}

/// Contributor to the repository from the GitHub API.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Contributor {
	/// The number of contributions they've made to the repository.
	///
	/// (It's not clear how GitHub counts this)
	contributions: i64,

	/// What "type" of user they are for the repository.
	///
	/// The example in the GitHub Docs only shows "User" as a possible value.
	/// I'm guessing this is a way of indicating who may be an administrator.
	#[serde(rename = "type")]
	contributor_type: String,

	/// The user's email.
	email: Option<String>,

	/// The user's username (GitHub calls them "logins").
	login: Option<String>,

	/// The user's display name.
	name: Option<String>,
}
