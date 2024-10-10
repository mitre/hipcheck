mod code_search;
mod data;
mod graphql;
mod types;
mod util;

use crate::data::GitHub;
use clap::Parser;
use hipcheck_sdk::{
	prelude::*,
	types::{KnownRemote, RemoteGitRepo},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::rc::Rc;
use std::result::Result as StdResult;
use std::sync::OnceLock;

struct Config {
	pub api_token: String,
}

#[derive(Deserialize)]
struct RawConfig {
	api_token_var: Option<String>,
}

impl TryFrom<RawConfig> for Config {
	type Error = ConfigError;
	fn try_from(value: RawConfig) -> StdResult<Config, ConfigError> {
		if let Some(atv) = value.api_token_var {
			let api_token =
				std::env::var(atv.as_str()).map_err(|_e| ConfigError::InvalidConfigValue {
					field_name: "api_token_var".to_owned(),
					value: atv,
					reason: "could not find an env var with that name".to_owned(),
				})?;
			Ok(Config { api_token })
		} else {
			Err(ConfigError::MissingRequiredConfig {
				field_name: "api_token_var".to_owned(),
				field_type: "name of env var containing GitHub API token".to_owned(),
				possible_values: vec![],
			})
		}
	}
}

static CONFIG: OnceLock<Config> = OnceLock::new();

#[derive(Debug, Serialize, JsonSchema, PartialEq, Eq)]
pub struct PullRequest {
	pub id: u64,
	pub reviews: u64,
}

fn get_github_agent<'a>(owner: &'a str, repo: &'a str) -> Result<GitHub<'a>> {
	GitHub::new(
		owner,
		repo,
		CONFIG
			.get()
			.ok_or_else(|| {
				log::error!("tried to access config before set by Hipcheck core!");
				Error::UnspecifiedQueryState
			})?
			.api_token
			.as_str(),
	)
	.map_err(|e| {
		log::error!("{}", e);
		Error::UnspecifiedQueryState
	})
}

#[query]
async fn pr_reviews(_engine: &mut PluginEngine, key: KnownRemote) -> Result<Vec<PullRequest>> {
	let (owner, repo) = match &key {
		KnownRemote::GitHub { owner, repo } => (owner, repo),
	};
	let results = get_github_agent(owner, repo)?
		.get_reviews_for_pr()
		.map_err(|e| {
			log::error!("{}", e);
			Error::UnspecifiedQueryState
		})?
		.into_iter()
		.map(|pr| PullRequest {
			id: pr.number,
			reviews: pr.reviews,
		})
		.collect();

	Ok(results)
}

#[query(default)]
async fn has_fuzz(_engine: &mut PluginEngine, key: RemoteGitRepo) -> Result<bool> {
	let (owner, repo) = match &key.known_remote {
		Some(KnownRemote::GitHub { owner, repo }) => (owner.as_str(), repo.as_str()),
		None => ("", ""),
	};
	let url = Rc::new(key.url.to_string());
	get_github_agent(owner, repo)?.fuzz_check(url).map_err(|e| {
		log::error!("{}", e);
		Error::UnspecifiedQueryState
	})
}

#[derive(Parser, Debug)]
struct Args {
	#[arg(long)]
	port: u16,
}

#[derive(Clone, Debug)]
struct GitlabAPIPlugin {}

impl Plugin for GitlabAPIPlugin {
	const PUBLISHER: &'static str = "mitre";
	const NAME: &'static str = "gitlab_api";

	fn set_config(&self, config: Value) -> StdResult<(), ConfigError> {
		let conf: Config = serde_json::from_value::<RawConfig>(config)
			.map_err(|e| ConfigError::Unspecified {
				message: e.to_string(),
			})?
			.try_into()?;
		CONFIG.set(conf).map_err(|_e| ConfigError::Unspecified {
			message: "config was already set".to_owned(),
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

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
	let args = Args::try_parse().unwrap();
	PluginServer::register(GitlabAPIPlugin {})
		.listen(args.port)
		.await
}
