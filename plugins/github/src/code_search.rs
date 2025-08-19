// SPDX-License-Identifier: Apache-2.0

use crate::util::authenticated_agent::AuthenticatedAgent;
use anyhow::{Result, anyhow};
use serde_json::Value;

const GH_API_V4_SEARCH: &str = "https://api.github.com/search/code";

/// Make a request to the GitHub Code Search API.
pub fn search_code_request(
	agent: &AuthenticatedAgent<'_>,
	repo: impl AsRef<String>,
) -> Result<bool> {
	// Example query will look like this:
	//     https://api.github.com/search/code?q=github.com%20assimp%20assimp+in:file+filename:project.yaml+repo:google/oss-fuzz
	//
	// Logic based on these docs:
	//     https://docs.github.com/en/search-github/searching-on-github/searching-code#considerations-for-code-search
	//
	// Breaking repo out in to more easily searchable string since full
	// GitHub repo urls were not working for a few fuzzed urls.

	let repo_query = repo
		.as_ref()
		.replace("https://", "")
		.replace("http://", "")
		.replace('/', "%20");

	let sub_query = format!(
		"{}+in:file+filename:project.yaml+repo:google/oss-fuzz",
		repo_query
	);

	let query = format!("{}?q={}", GH_API_V4_SEARCH.to_owned(), sub_query);

	// Make the get request.
	let json = get_request(agent, query).map_err(|_| anyhow!("unable to query fuzzing info"))?;

	match &json["total_count"].to_string().parse::<u64>() {
		Ok(count) => Ok(count > &0),
		_ => Err(anyhow!("unable to get fuzzing status")),
	}
}

/// Get call using agent
fn get_request(agent: &AuthenticatedAgent<'_>, query: String) -> Result<Value> {
	let response = agent.get(&query).call()?.into_json()?;
	Ok(response)
}
