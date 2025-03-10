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
use anyhow::{anyhow, Context as _};
use clap::Parser;
use hipcheck_sdk::{macros::PluginConfig, prelude::*, types::Target, LogLevel, PluginConfig};
use std::{path::PathBuf, result::Result as StdResult, sync::OnceLock};

pub static TYPOFILE: OnceLock<TypoFile> = OnceLock::new();

#[derive(PluginConfig, Debug)]
struct Config {
	typo_file: PathBuf,
	count_threshold: Option<u64>,
}

#[query(default)]
async fn typo(engine: &mut PluginEngine, value: Target) -> Result<Vec<bool>> {
	tracing::info!("running typo query");

	// Get the typo file.
	let typo_file = TYPOFILE
		.get()
		.ok_or_else(|| anyhow!("could not find typo file"))?;

	// Get the repo's dependencies
	let value = engine
		.query("mitre/npm/dependencies", value.local)
		.await
		.context("failed to get dependencies")?;

	let dependencies: NpmDependencies =
		serde_json::from_value(value).map_err(Error::InvalidJsonInQueryOutput)?;

	// Get the dependencies with identified typos
	let typo_deps = match dependencies.language {
		Lang::JavaScript => languages::typos_for_javascript(typo_file, dependencies.clone())?,
		Lang::Unknown => Err(anyhow!("failed to identify a known language"))?,
	};

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

	tracing::info!("completed typo query");
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
		let conf = Config::deserialize(&config)?;

		// Parse typo TOML file
		let typo_file = TypoFile::load_from(&conf.typo_file).map_err(|e| {
			// Print error with Debug for full context
			tracing::error!("{:?}", e);
			ConfigError::ParseError {
				source: format!("Typo file at path {}", conf.typo_file.display()),
				// Print error with Debug for full context
				message: format!("{:?}", e),
			}
		})?;

		// Store the policy conf to be accessed only in the `default_policy_expr()` impl
		self.policy_conf
			.set(conf.count_threshold)
			.map_err(|_| ConfigError::InternalError {
				message: "plugin was already configured".to_string(),
			})?;

		TYPOFILE
			.set(typo_file)
			.map_err(|_e| ConfigError::InternalError {
				message: "config was already set".to_owned(),
			})
	}

	fn default_policy_expr(&self) -> Result<String> {
		let conf = self.policy_conf.get().ok_or(Error::UnspecifiedQueryState)?;
		let threshold = conf.unwrap_or(0);
		Ok(format!("(lte (count (filter (eq #t) $)) {})", threshold))
	}

	fn explain_default_query(&self) -> Result<Option<String>> {
		Ok(Some(
			"the repository's dependencies flagged as typos".to_string(),
		))
	}

	queries! {}
}

#[derive(Parser, Debug)]
struct Args {
	#[arg(long)]
	port: u16,

	#[arg(long, default_value_t=LogLevel::Error)]
	log_level: LogLevel,

	#[arg(trailing_var_arg(true), allow_hyphen_values(true), hide = true)]
	unknown_args: Vec<String>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
	let args = Args::try_parse().unwrap();
	PluginServer::register(TypoPlugin::default(), args.log_level)
		.listen_local(args.port)
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
			.insert("mitre/npm/dependencies", local, Ok(output))
			.unwrap();

		Ok(mock_responses)
	}

	#[tokio::test]
	async fn test_typo() {
		let typo_path = pathbuf![&env::current_dir().unwrap(), "test", "Typos.kdl"];
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

		let concerns = engine.get_concerns();
		assert!(concerns.contains(&"chakl".to_string()));
		assert!(concerns.contains(&"reacct".to_string()));
	}
}
