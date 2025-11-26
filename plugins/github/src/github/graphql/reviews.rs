// SPDX-License-Identifier: Apache-2.0

use self::reviews::{ResponseData, ReviewsRepositoryPullRequestsNodes as RawPull, Variables};
use crate::{github::graphql::GH_API_V4, tls::authenticated_agent::AuthenticatedAgent};
use anyhow::{Result, anyhow};
use graphql_client::{GraphQLQuery, QueryBody, Response};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{from_value as from_json_value, to_value as to_json_value};
use std::convert::TryInto;

/// Defines the query being made against the GitHub API.
#[derive(GraphQLQuery)]
#[graphql(
	schema_path = "src/github/graphql/schemas/types.graphql",
	query_path = "src/github/graphql/schemas/reviews.graphql",
	response_derives = "Debug",
	custom_scalars_module = "crate::github::graphql::custom_scalars"
)]
pub struct Reviews;

/// Query the GitHub GraphQL API for reviews performed on PRs for a repo.
pub fn get_reviews(
	agent: &AuthenticatedAgent<'_>,
	owner: &str,
	repo: &str,
) -> Result<Vec<GitHubPullRequest>> {
	let vars = Vars::new(owner, repo);

	let mut data = Vec::new();
	let mut cursor = None;

	// Keep making requests so long as there's cursor data indicating more
	// requests need to be made.
	while let new_cursor @ Some(_) = get_reviews_inner(agent, vars.with_cursor(cursor), &mut data)?
	{
		cursor = new_cursor;
	}

	Ok(data)
}

/// Convenience struct for creating the `Variables` struct needed for a query.
struct Vars<'a> {
	owner: &'a str,
	repo: &'a str,
}

impl<'a> Vars<'a> {
	/// Construct a new `Vars` for the given owner and repo.
	fn new(owner: &'a str, repo: &'a str) -> Vars<'a> {
		Vars { owner, repo }
	}

	/// Generate `Variables` with the given cursor.
	fn with_cursor(&self, cursor: Option<String>) -> Variables {
		Variables {
			owner: self.owner.to_owned(),
			repo: self.repo.to_owned(),
			cursor,
		}
	}
}

/// Convenient shorthand for a cursor from the GitHub API.
type Cursor = Option<String>;

/// Query the GitHub GraphQL API for reviews performed on PRs for a repo.
fn get_reviews_inner(
	agent: &AuthenticatedAgent<'_>,
	variables: Variables,
	data: &mut Vec<GitHubPullRequest>,
) -> Result<Cursor> {
	// Setup the query.
	let query = Reviews::build_query(variables);

	// Make the request.
	let body = make_request(agent, query)?;

	// Get the cursor, if there is one.
	let cursor = get_cursor(&body);

	// Process and collect all the PRs.
	data.extend(get_prs(body)?.into_iter().map(process_pr));

	Ok(cursor)
}

/// Make a request to the GitHub API.
fn make_request(
	agent: &AuthenticatedAgent<'_>,
	query: QueryBody<Variables>,
) -> Result<Response<ResponseData>> {
	let response = agent.post(GH_API_V4).send_json(to_json_value(query)?)?;
	if response.status() == 200 {
		return Ok(from_json_value(response.into_json()?)?);
	}
	Err(anyhow!(
		"request to GitHub API returned the following HTTP status: {} {}",
		response.status(),
		response.status_text()
	))
}

/// Get the cursor, if there is one.
fn get_cursor(body: &Response<ResponseData>) -> Cursor {
	let page_info = &body
		.data
		.as_ref()?
		.repository
		.as_ref()?
		.pull_requests
		.page_info;

	if page_info.has_next_page {
		page_info.end_cursor.clone()
	} else {
		None
	}
}

/// Extract any PRs from the GitHub API response data.
fn get_prs(body: Response<ResponseData>) -> Result<Vec<RawPull>> {
	// Get the repo info from the response.
	let prs = body
		.data
		.ok_or_else(|| anyhow!("missing response data from GitHub"))?
		.repository
		.ok_or_else(|| anyhow!("repository not found on GitHub"))?
		.pull_requests
		.nodes;

	match prs {
		None => Ok(Vec::new()),
		// Convert Vec<Option<_>> into Option<Vec<_>>.
		Some(prs) => match prs.into_iter().collect() {
			None => Ok(Vec::new()),
			Some(prs) => Ok(prs),
		},
	}
}

/// Convert a single RawPull to a GitHubPullRequest
fn process_pr(pr: RawPull) -> GitHubPullRequest {
	let id: u64 = pr.number.try_into().unwrap();
	let reviews: u64 = match pr.reviews {
		None => 0,
		Some(reviews) => match reviews.nodes {
			None => 0,
			Some(reviews) => reviews.len(),
		},
	}
	.try_into()
	.unwrap();

	GitHubPullRequest { id, reviews }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct GitHubPullRequest {
	pub id: u64,
	pub reviews: u64,
}
