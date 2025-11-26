// SPDX-License-Identifier: Apache-2.0

use crate::tls::authenticated_agent::AuthenticatedAgent;
use anyhow::{Result, anyhow};
use serde_json::Value;

/// Endpoint for GitHub Code Search.
const CODE_SEARCH: &str = "https://api.github.com/search/code";

/// Check if the given repo participates in OSS-Fuzz.
pub fn check_fuzzing(agent: &AuthenticatedAgent<'_>, repo: &str) -> Result<bool> {
	// Example query will look like this:
	//     https://api.github.com/search/code?q=github.com%20assimp%20assimp+in:file+filename:project.yaml+repo:google/oss-fuzz
	//
	// Logic based on these docs:
	//     https://docs.github.com/en/search-github/searching-on-github/searching-code#considerations-for-code-search
	//
	// Breaking repo out in to more easily searchable string since full
	// GitHub repo urls were not working for a few fuzzed urls.

	let repo = repo
		.replace("https://", "")
		.replace("http://", "")
		.replace('/', "%20");

	let query = format!(
		"{}?q={}+in:file+filename:project.yaml+repo:google/oss-fuzz",
		CODE_SEARCH, repo
	);

	let json = agent
		.get(&query)
		.call()?
		.into_json::<Value>()
		.map_err(|_| anyhow!("unable to query fuzzing info"))?;

	let count = json["total_count"]
		.to_string()
		.parse::<u64>()
		.map_err(|_| anyhow!("unable to parse count"))?;

	Ok(count > 0)
}
