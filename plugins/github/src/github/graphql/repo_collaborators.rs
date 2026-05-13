// SPDX-License-Identifier: Apache-2.0

use self::repo_collaborators::{ResponseData, Variables};
use crate::{github::graphql::GH_API_V4, tls::authenticated_agent::AuthenticatedAgent};
use anyhow::{Result, anyhow};
use graphql_client::{GraphQLQuery, QueryBody, Response};
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::{from_value as from_json_value, to_value as to_json_value};

/// Defines the query being made against the GitHub API.
#[derive(GraphQLQuery)]
#[graphql(
	schema_path = "src/github/graphql/schemas/types.graphql",
	query_path = "src/github/graphql/schemas/repo_collaborators.graphql",
	response_derives = "Debug",
	custom_scalars_module = "crate::github::graphql::custom_scalars"
)]
pub struct RepoCollaborators;

/// Get organization data for the target user.
pub fn get_repo_collaborators(
	agent: &AuthenticatedAgent<'_>,
	owner: &str,
	name: &str,
) -> Result<Vec<Collaborator>> {
	let query = RepoCollaborators::build_query(Variables {
		owner: owner.to_owned(),
		name: name.to_owned(),
	});

	let body = make_request(agent, query)?;

	if let Some(errors) = body.errors {
		let mut error_string = String::new();
		for error in errors {
			error_string.push_str(&format!("{error}"));
			error_string.push('\n');
		}
		error_string.pop().unwrap();
		tracing::error!("Error string: {error_string}");
		return Err(anyhow!(
			"mitre/github/repo_collaborators, error here from GitHub API: {error_string}"
		));
	}

	let repository = body
		.data
		.ok_or_else(|| anyhow!("missing response data from GitHub"))?
		.repository
		.ok_or_else(|| anyhow!("repository not found on GitHub"))?;

	let Some(collaborators) = repository.collaborators else {
		return Err(anyhow!("no collaborators for repository"));
	};

	let Some(collaborators) = collaborators.nodes else {
		return Err(anyhow!("no collaborators for repository"));
	};

	let collaborators = collaborators
		.into_iter()
		.filter_map(|user| {
			// Filter out invalid users.
			let user = user?;

			let login = user.login;
			let email = user.email;
			let profile_employer = user.company;

			let github_orgs = user
				.organizations
				.nodes
				.unwrap_or_default()
				.into_iter()
				.filter_map(|org| {
					let org = org?;

					Some(GitHubOrg {
						name: org.name,
						login: org.login,
						email: org.email,
					})
				})
				.collect();

			Some(Collaborator {
				login,
				email,
				profile_employer,
				github_orgs,
			})
		})
		.collect();

	Ok(collaborators)
}

/// Make a request to the GitHub API.
fn make_request(
	agent: &AuthenticatedAgent<'_>,
	query: QueryBody<Variables>,
) -> Result<Response<ResponseData>> {
	let response = {
		let request_json = to_json_value(query)?;
		agent.post(GH_API_V4).send_json(request_json)?
	};

	match response.status() {
		200 => {
			let response_json = response.into_json()?;
			let parsed = from_json_value(response_json)?;
			Ok(parsed)
		}
		status => Err(anyhow!(
			"request to GitHub API returned the following HTTP status: {} {}",
			status,
			response.status_text()
		)),
	}
}

/// Repository collaborator data pulled from the GitHub API.
#[derive(Debug, Default, Serialize, JsonSchema)]
pub struct Collaborator {
	/// The user's username.
	pub login: String,
	/// The user's public email address.
	pub email: String,
	/// The user's employer, as pulled from their GitHub profile.
	pub profile_employer: Option<String>,
	/// The GitHub organizations the user belongs to.
	pub github_orgs: Vec<GitHubOrg>,
}

/// A single organization on GitHub.
#[derive(Debug, Serialize, JsonSchema)]
pub struct GitHubOrg {
	/// The display name of the organization, if present.
	pub name: Option<String>,
	/// The username of the organization.
	pub login: String,
	/// The organization's public email, if present.
	pub email: Option<String>,
}
