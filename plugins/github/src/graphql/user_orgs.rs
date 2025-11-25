// SPDX-License-Identifier: Apache-2.0

use self::user_orgs::{ResponseData, Variables};
use crate::{graphql::GH_API_V4, tls::authenticated_agent::AuthenticatedAgent};
use anyhow::{Result, anyhow};
use graphql_client::{GraphQLQuery, QueryBody, Response};
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::{from_value as from_json_value, to_value as to_json_value};
use url::Url;

/// Defines the query being made against the GitHub API.
#[derive(GraphQLQuery)]
#[graphql(
	schema_path = "src/graphql/schemas/types.graphql",
	query_path = "src/graphql/schemas/user_orgs.graphql",
	response_derives = "Debug",
	custom_scalars_module = "crate::graphql::custom_scalars"
)]
pub struct UserOrgs;

/// Get organization data for the target user.
pub fn get_user_orgs(agent: &AuthenticatedAgent<'_>, login: &str) -> Result<UserOrgData> {
	let query = UserOrgs::build_query(Variables {
		login: login.to_owned(),
	});

	let body = make_request(agent, query)?;

	let user = body
		.data
		.ok_or_else(|| anyhow!("missing response data from GitHub"))?
		.user
		.ok_or_else(|| anyhow!("user not found on GitHub"))?;

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
				domains: org
					.domains?
					.nodes
					.unwrap_or_default()
					.into_iter()
					.filter_map(|domain| {
						let domain = domain?;

						Some(GitHubOrgDomain {
							domain: domain.domain,
							is_approved: domain.is_approved,
							is_verified: domain.is_verified,
						})
					})
					.collect(),
			})
		})
		.collect();

	Ok(UserOrgData {
		profile_employer,
		github_orgs,
	})
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

/// User's organization membership data pulled from the GitHub API.
#[derive(Debug, Default, Serialize, JsonSchema)]
pub struct UserOrgData {
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
	/// Domain names associated with the organization.
	pub domains: Vec<GitHubOrgDomain>,
}

/// A single domain associated with a GitHub organization.
#[derive(Debug, Serialize, JsonSchema)]
pub struct GitHubOrgDomain {
	/// The domain name associated with the organization.
	pub domain: Url,
	/// Whether the domain has been approved by the organization.
	pub is_approved: bool,
	/// Whether the domain has been verified by the organization.
	pub is_verified: bool,
}
