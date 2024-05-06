// SPDX-License-Identifier: Apache-2.0

use self::review::{
	ResponseData, ReviewRepositoryPullRequest as RawPull,
	ReviewRepositoryPullRequestCommitsNodes as RawPullCommit, Variables,
};
use crate::data::{
	git::{Contributor, RawCommit},
	github::{authenticated_agent::AuthenticatedAgent, data::*},
};
use crate::{
	chrono::DateTime,
	context::Context,
	error::{Error, Result},
	hc_error,
	serde_json::{from_value as from_json_value, to_value as to_json_value},
};
use graphql_client::{GraphQLQuery, QueryBody, Response};
use std::convert::TryFrom;

/// The URL of the GitHub GraphQL API.
const GH_API_V4: &str = "https://api.github.com/graphql";

/// Defines the query being made against the GitHub API.
#[derive(GraphQLQuery)]
#[graphql(
	schema_path = "src/data/github/gh_schema.graphql",
	query_path = "src/data/github/gh_query.graphql",
	response_derives = "Debug"
)]
pub struct Review;

/// Query the GitHub GraphQL API for review performed on the given PR.
pub fn get_all_pr_reviews(
	agent: &AuthenticatedAgent<'_>,
	owner: &str,
	repo: &str,
	number: &i64,
) -> Result<GitHubPullRequestWithCommits> {
	let vars = Vars::new(owner, repo, number);

	let mut commits = Vec::new();
	let mut cursor = None;

	// Keep making requests so long as there's cursor data indicating more
	// requests need to be made.
	while let new_cursor @ Some(_) = get_all_commits(agent, vars.with_cursor(cursor), &mut commits)?
	{
		cursor = new_cursor;
	}

	let pull_request = get_review(agent, vars.without_cursor())?;

	Ok(GitHubPullRequestWithCommits {
		pull_request,
		commits,
	})
}

/// Convenience struct for creating the `Variables` struct needed for a query.
struct Vars<'a> {
	owner: &'a str,
	repo: &'a str,
	number: &'a i64,
}

impl<'a> Vars<'a> {
	/// Construct a new `Vars` for the given owner and repo.
	fn new(owner: &'a str, repo: &'a str, number: &'a i64) -> Vars<'a> {
		Vars {
			owner,
			repo,
			number,
		}
	}

	/// Generate `Variables` without a cursor.
	fn without_cursor(&self) -> Variables {
		Variables {
			owner: self.owner.to_owned(),
			repo: self.repo.to_owned(),
			number: self.number.to_owned(),
			cursor: None,
		}
	}

	/// Generate `Variables` with the given cursor.
	fn with_cursor(&self, cursor: Option<String>) -> Variables {
		Variables {
			owner: self.owner.to_owned(),
			repo: self.repo.to_owned(),
			number: self.number.to_owned(),
			cursor,
		}
	}
}

/// Convenient shorthand for a cursor from the GitHub API.
type Cursor = Option<String>;

/// Query the GitHub GraphQL API for reviews performed on PRs for a repo.
fn get_all_commits(
	agent: &AuthenticatedAgent<'_>,
	variables: Variables,
	commits: &mut Vec<RawCommit>,
) -> Result<Cursor> {
	// Setup the query.
	let query = Review::build_query(variables);

	// Make the request.
	let body = make_request(agent, query)?;

	// Get the cursor, if there is one.
	let cursor = get_cursor(&body);

	// Process and collect all the commits.
	commits.extend(get_commits(body)?.into_iter().map(process_commit));

	Ok(cursor)
}

fn get_review(agent: &AuthenticatedAgent<'_>, variables: Variables) -> Result<GitHubPullRequest> {
	// Setup the query.
	let query = Review::build_query(variables);

	// Make the request.
	let body = make_request(agent, query)?;

	let pr = get_pr(body)?;

	process_pr(pr)
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
	Err(hc_error!(
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
		.pull_request
		.as_ref()?
		.commits
		.as_ref()?
		.page_info;

	if page_info.has_next_page {
		page_info.end_cursor.clone()
	} else {
		None
	}
}

/// Extract any commits from the GitHub API response data.
fn get_commits(body: Response<ResponseData>) -> Result<Vec<RawPullCommit>> {
	// Get the repo info from the response.
	let commits = body
		.data
		.ok_or_else(|| Error::msg("missing response data from GitHub"))?
		.repository
		.ok_or_else(|| Error::msg("repository not found on GitHub"))?
		.pull_request
		.ok_or_else(|| Error::msg("pull request not found on GitHub"))?
		.commits
		.ok_or_else(|| Error::msg("pull request commits not found on GitHub"))?
		.nodes;

	match commits {
		None => Ok(Vec::new()),
		// Convert Vec<Option<_>> into Option<Vec<_>>.
		Some(commits) => match commits.into_iter().collect() {
			None => Ok(Vec::new()),
			Some(commits) => Ok(commits),
		},
	}
}

/// Convert a single RawPullCommit to a RawCommit
fn process_commit(raw_commit: RawPullCommit) -> RawCommit {
	let hash: String = raw_commit.commit.oid;

	let author = match raw_commit.commit.author {
		Some(author) => Contributor {
			name: match author.name {
				Some(name) => name,
				None => "".to_string(),
			},
			email: match author.email {
				Some(email) => email,
				None => "".to_string(),
			},
		},
		None => Contributor {
			name: "".to_string(),
			email: "".to_string(),
		},
	};
	let written_on =
		DateTime::parse_from_rfc3339(&raw_commit.commit.authored_date).map_err(|e| e.to_string());

	let committer = match raw_commit.commit.committer {
		Some(committer) => Contributor {
			name: match committer.name {
				Some(name) => name,
				None => "".to_string(),
			},
			email: match committer.email {
				Some(email) => email,
				None => "".to_string(),
			},
		},
		None => Contributor {
			name: "".to_string(),
			email: "".to_string(),
		},
	};
	let committed_on =
		DateTime::parse_from_rfc3339(&raw_commit.commit.committed_date).map_err(|e| e.to_string());

	let mut signer_name: Option<String> = None;
	let mut signer_key: Option<String> = None;

	if let Some(signature) = raw_commit.commit.signature {
		if signature.is_valid {
			signer_name = match signature.signer {
				Some(signer) => signer.name,
				None => None,
			};
			signer_key = Some(signature.signature);
		}
	}

	RawCommit {
		hash,
		author,
		written_on,
		committer,
		committed_on,
		signer_name,
		signer_key,
	}
}

/// Extract PR from the GitHub API response data.
fn get_pr(body: Response<ResponseData>) -> Result<RawPull> {
	// Get the repo info from the response.
	let pr = body
		.data
		.ok_or_else(|| Error::msg("missing response data from GitHub"))?
		.repository
		.ok_or_else(|| Error::msg("repository not found on GitHub"))?
		.pull_request
		.ok_or_else(|| Error::msg("no pull request with that number found"))?;

	Ok(pr)
}

/// Convert a single RawPull to a GitHubPullRequest
fn process_pr(pr: RawPull) -> Result<GitHubPullRequest> {
	// Convert the pull request number and the review count to an unsigned integers when we retrieve them here
	let number: u64 = u64::try_from(pr.number)
		.context("pull request number is not a non-zero positive integer")?;
	let reviews: u64 = u64::try_from(match pr.reviews {
		None => 0,
		Some(reviews) => match reviews.nodes {
			None => 0,
			Some(reviews) => reviews.len(),
		},
	})
	.context("there are a non-positive integer number of pull request reviews")?;

	Ok(GitHubPullRequest { number, reviews })
}
