// SPDX-License-Identifier: Apache-2.0

//! Plugin for querying typos were found in the repository's package dependencies
//! Currently only NPM dependencies for JavaScript repositories are supported

mod languages;
mod types;
mod util;

use crate::{
	languages::TypoFile,
	types::{Lang, NpmDependencies},
};
use clap::Parser;
use hipcheck_sdk::{prelude::*, types::Target};
use serde::Deserialize;
use std::{path::PathBuf, result::Result as StdResult, sync::OnceLock};

pub static TYPOFILE: OnceLock<TypoFile> = OnceLock::new();

#[derive(Deserialize)]
struct RawConfig {
	#[serde(rename = "typo-file-path")]
	typo_file_path: Option<String>,
	#[serde(rename = "count-threshold")]
	count_threshold: Option<u64>,
}

struct Config {
	typo_file: TypoFile,
	count_threshold: Option<u64>,
}

impl TryFrom<RawConfig> for Config {
	type Error = hipcheck_sdk::error::ConfigError;
	fn try_from(value: RawConfig) -> StdResult<Config, Self::Error> {
		// Get path to typo TOML file
		let Some(raw_typo_path) = value.typo_file_path else {
			return Err(ConfigError::MissingRequiredConfig {
				field_name: "typo_file_path".to_owned(),
				field_type: "string".to_owned(),
				possible_values: vec![],
			});
		};
		// Parse typo TOML file
		let typo_path = PathBuf::from(raw_typo_path);
		let typo_file = TypoFile::load_from(&typo_path).map_err(|e| {
			log::error!("failed to load typo file: {}", e);
			ConfigError::InvalidConfigValue {
				field_name: "typo_file_path".to_owned(),
				value: "string".to_owned(),
				reason: format!("failed to load typo file: {}", e),
			}
		})?;

		let count_threshold = value.count_threshold;

		Ok(Config {
			typo_file,
			count_threshold,
		})
	}
}

#[query(default)]
async fn typo(engine: &mut PluginEngine, value: Target) -> Result<Vec<bool>> {
	log::debug!("running typo query");

	let local = value.local;

	// Get the typo file
	let typo_file = TYPOFILE.get().ok_or(Error::UnspecifiedQueryState)?;

	// Get the repo's dependencies
	let value = engine
		.query("mitre/npm_dependencies/dependencies", local)
		.await
		.map_err(|e| {
			log::error!("failed to get dependencies for typo query: {}", e);
			Error::UnspecifiedQueryState
		})?;
	let dependencies: NpmDependencies =
		serde_json::from_value(value).map_err(Error::InvalidJsonInQueryOutput)?;

	// Get the dependencies with identified typos
	let typo_deps = match dependencies.language {
		Lang::JavaScript => languages::typos_for_javascript(typo_file, dependencies.clone())
			.map_err(|e| {
				log::error!("{}", e);
				Error::UnspecifiedQueryState
			}),
		Lang::Unknown => {
			log::error!("failed to identify a known language");
			Err(Error::UnexpectedPluginQueryInputFormat)
		}
	}?;

	// Generate a boolean list of depedencies with and without typos
	let typos = dependencies
		.deps
		.iter()
		.map(|d| typo_deps.contains(d))
		.collect();

	// Report each dependency typo as a concern
	for concern in typo_deps {
		engine.record_concern(concern);
	}

	log::info!("completed typo query");

	Ok(typos)
}

#[derive(Clone, Debug, Default)]
struct TypoPlugin {
	policy_conf: OnceLock<Option<u64>>,
}

impl Plugin for TypoPlugin {
	const PUBLISHER: &'static str = "mitre";

	const NAME: &'static str = "typo";

	fn set_config(&self, config: Value) -> StdResult<(), ConfigError> {
		// Deserialize and validate the config struct
		let conf: Config = serde_json::from_value::<RawConfig>(config)
			.map_err(|e| ConfigError::Unspecified {
				message: e.to_string(),
			})?
			.try_into()?;

		// Store the policy conf to be accessed only in the `default_policy_expr()` impl
		self.policy_conf
			.set(conf.count_threshold)
			.map_err(|_| ConfigError::Unspecified {
				message: "plugin was already configured".to_string(),
			})?;

		TYPOFILE
			.set(conf.typo_file)
			.map_err(|_e| ConfigError::Unspecified {
				message: "config was already set".to_owned(),
			})
	}

	fn default_policy_expr(&self) -> Result<String> {
		match self.policy_conf.get() {
			None => Err(Error::UnspecifiedQueryState),
			// If no policy vars, we have no default expr
			Some(None) => Ok("".to_owned()),
			// Use policy config vars to construct a default expr
			Some(Some(policy_conf)) => {
				Ok(format!("(lte (count (filter (eq #t) $)) {})", policy_conf))
			}
		}
	}

	fn explain_default_query(&self) -> Result<Option<String>> {
		Ok(Some(
			"Returns whether each of the repository's package dependencies has a typo in its name"
				.to_string(),
		))
	}

	queries! {}
}

#[derive(Parser, Debug)]
struct Args {
	#[arg(long)]
	port: u16,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
	let args = Args::try_parse().unwrap();
	PluginServer::register(TypoPlugin::default())
		.listen(args.port)
		.await
}

#[cfg(test)]
mod test {
	use super::*;

	use hipcheck_sdk::types::LocalGitRepo;
	use pathbuf::pathbuf;
	use std::env;

	fn local() -> LocalGitRepo {
		LocalGitRepo {
			path: "/home/users/me/.cache/hipcheck/clones/github/foo/bar/".to_string(),
			git_ref: "main".to_string(),
		}
	}

	fn mock_responses() -> StdResult<MockResponses, Error> {
		let local = local();

		let deps = vec![
			"lodash".to_string(),
			"chakl".to_string(),
			"reacct".to_string(),
		];
		let output = NpmDependencies {
			language: Lang::JavaScript,
			deps,
		};

		let mut mock_responses = MockResponses::new();
		mock_responses
			.insert("mitre/npm_dependencies/dependencies", local, Ok(output))
			.unwrap();

		Ok(mock_responses)
	}

	#[tokio::test]
	async fn test_typo() {
		let typo_path = pathbuf![&env::current_dir().unwrap(), "test", "Typos.toml"];
		let typo_file = TypoFile::load_from(&typo_path).unwrap();
		TYPOFILE.get_or_init(|| typo_file);

		let local = local();
		let target = Target {
			specifier: "bar".to_string(),
			local,
			remote: None,
			package: None,
		};

		let mut engine = PluginEngine::mock(mock_responses().unwrap());
		let output = typo(&mut engine, target).await.unwrap();
		assert_eq!(output.len(), 3);
		let num_typos = output.iter().filter(|&n| *n).count();
		assert_eq!(num_typos, 2);

		let concerns = engine.take_concerns();
		assert!(concerns.contains(&"chakl".to_string()));
		assert!(concerns.contains(&"reacct".to_string()));
	}
}
