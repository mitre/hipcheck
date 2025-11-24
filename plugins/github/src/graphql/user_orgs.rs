// SPDX-License-Identifier: Apache-2.0

#![allow(unused)]

use self::user_orgs::{ResponseData, Variables};
use crate::{graphql::GH_API_V4, tls::authenticated_agent::AuthenticatedAgent};
use anyhow::{Result, anyhow};
use graphql_client::{GraphQLQuery, QueryBody, Response};
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

	let data = UserOrgData {
		profile_employer: user.company,
		github_orgs: user
			.organizations
			.nodes
			.map(|nodes| {
				nodes
					.into_iter()
					.filter_map(|opt_node| {
						opt_node.map(|node| GitHubOrg {
							name: node.name,
							login: node.login,
							domains: node
								.domains
								.map(|domains| {
									domains
										.nodes
										.map(|domains| {
											domains
												.into_iter()
												.filter_map(|domain| {
													domain.map(|domain| GitHubOrgDomain {
														domain: domain.domain,
														is_approved: domain.is_approved,
														is_verified: domain.is_verified,
													})
												})
												.collect::<Vec<_>>()
										})
										.unwrap_or_else(|| vec![])
								})
								.unwrap_or_else(|| vec![]),
						})
					})
					.collect::<Vec<_>>()
			})
			.unwrap_or_else(|| vec![]),
	};

	Ok(data)
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
		status @ _ => Err(anyhow!(
			"request to GitHub API returned the following HTTP status: {} {}",
			status,
			response.status_text()
		)),
	}
}

#[derive(Debug, Default)]
pub struct UserOrgData {
	pub profile_employer: Option<String>,
	pub github_orgs: Vec<GitHubOrg>,
}

#[derive(Debug)]
pub struct GitHubOrg {
	pub name: Option<String>,
	pub login: String,
	pub domains: Vec<GitHubOrgDomain>,
}

#[derive(Debug)]
pub struct GitHubOrgDomain {
	pub domain: Url,
	pub is_approved: bool,
	pub is_verified: bool,
}
