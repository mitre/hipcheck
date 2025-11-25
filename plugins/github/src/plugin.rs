// SPDX-License-Identifier: Apache-2.0

use crate::api::GitHub;
use crate::config::{CONFIG, Config, RawConfig};
use crate::graphql::user_orgs::UserOrgData;
use hipcheck_sdk::{
	prelude::*,
	types::{KnownRemote, RemoteGitRepo},
};
use schemars::JsonSchema;
use serde::Serialize;
use std::result::Result as StdResult;
use std::{fmt::Display, rc::Rc};

/// Logs the error and returns a generic query error, to adapt errors for use
/// across the plugin boundary.
fn query_err<E: Display>(e: E) -> Error {
	tracing::error!("{}", e);
	Error::UnspecifiedQueryState
}

/// Reads the API token from the configuration.
fn get_api_token() -> Result<&'static str> {
	Ok(CONFIG
		.get()
		.ok_or_else(|| {
			tracing::error!("tried to access config before set by Hipcheck core!");
			Error::UnspecifiedQueryState
		})?
		.api_token
		.as_str())
}

#[query]
async fn pr_reviews(_engine: &mut PluginEngine, key: KnownRemote) -> Result<Vec<PullRequest>> {
	tracing::info!("running pr_reviews query");

	let (owner, repo) = match &key {
		KnownRemote::GitHub { owner, repo } => (owner, repo),
	};

	let api_token = get_api_token()?;

	let results = GitHub::new(api_token)
		.map_err(query_err)?
		.get_reviews_for_pr(owner, repo)
		.map_err(query_err)?
		.into_iter()
		.map(|pr| PullRequest {
			id: pr.number,
			reviews: pr.reviews,
		})
		.collect();

	tracing::info!("completed pr_reviews query");

	Ok(results)
}

#[query]
async fn has_fuzz(_engine: &mut PluginEngine, key: RemoteGitRepo) -> Result<bool> {
	tracing::info!("running has_fuzz query");

	let api_token = get_api_token()?;

	let uses_oss_fuzz = GitHub::new(api_token)
		.map_err(query_err)?
		.fuzz_check(Rc::new(key.url.to_string()))
		.map_err(query_err)?;

	tracing::info!("completed has_fuzz query");

	Ok(uses_oss_fuzz)
}

#[query]
async fn user_orgs(_engine: &mut PluginEngine, user_login: String) -> Result<UserOrgData> {
	tracing::info!("running user_org query");
	let api_token = get_api_token()?;
	let agent = GitHub::new(api_token)?;
	let user_orgs = agent.get_user_orgs(&user_login)?;
	tracing::info!("completed user_orgs query");
	Ok(user_orgs)
}

#[derive(Clone, Debug)]
pub struct GithubAPIPlugin {}

impl GithubAPIPlugin {
	pub fn new() -> Self {
		GithubAPIPlugin {}
	}
}

impl Plugin for GithubAPIPlugin {
	const PUBLISHER: &'static str = "mitre";
	const NAME: &'static str = "github";

	fn set_config(&self, config: Value) -> StdResult<(), ConfigError> {
		let conf: Config = serde_json::from_value::<RawConfig>(config)
			.map_err(|e| ConfigError::Unspecified {
				message: e.to_string().into_boxed_str(),
			})?
			.try_into()?;

		CONFIG.set(conf).map_err(|_e| ConfigError::InternalError {
			message: "config was already set".to_owned().into_boxed_str(),
		})
	}

	fn default_policy_expr(&self) -> Result<String> {
		Ok("".to_owned())
	}

	fn explain_default_query(&self) -> Result<Option<String>> {
		Ok(None)
	}

	queries! {}
}

#[derive(Debug, Serialize, JsonSchema, PartialEq, Eq)]
pub struct PullRequest {
	pub id: u64,
	pub reviews: u64,
}
