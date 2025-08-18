// SPDX-License-Identifier: Apache-2.0

mod npm;
mod util;

use crate::{
	npm::{get_dependencies, get_npm_version},
	util::command::check_version,
};
use clap::Parser;
use hipcheck_sdk::{prelude::*, LogLevel};
use pathbuf::pathbuf;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A locally stored git repo
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LocalGitRepo {
	/// The path to the repo.
	pub path: PathBuf,

	/// The Git ref we're referring to.
	pub git_ref: String,
}

/// Information about a package's language and dependencies
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NpmDependencies {
	/// The package language
	pub language: Lang,
	/// A list of the package's dependencies
	pub deps: Vec<String>,
}

impl NpmDependencies {
	/// Get the NPM dependencies given the path to the repo and the NPM version, after confirming that there is a package.json file
	pub fn resolve(repo: &Path, version: String) -> Result<NpmDependencies> {
		match Lang::detect(repo) {
			language @ Lang::JavaScript => {
				let deps = get_dependencies(repo, version)
					.map_err(|e| {
						tracing::error!("{}", e);
						Error::UnspecifiedQueryState
					})?
					.into_iter()
					.collect();
				Ok(NpmDependencies { language, deps })
			}
			Lang::Unknown => {
				tracing::error!("can't identify a known language in the repository");
				Err(Error::UnspecifiedQueryState)
			}
		}
	}
}

/// Supported languages for dependency checking.
///
/// Because we are looking for NPM dependencies, the only supported language is JavaScript.
#[derive(Debug, PartialEq, Eq, Copy, Clone, Serialize, Deserialize, JsonSchema)]
pub enum Lang {
	JavaScript,
	Unknown,
}

impl Lang {
	/// Confirm the repo contains Javascript by looking for a `package.json` file
	fn detect(repo: &Path) -> Lang {
		if pathbuf![repo, "package.json"].exists() {
			Lang::JavaScript
		} else {
			Lang::Unknown
		}
	}
}

/// Returns the NPM dependencies for the repo
#[query]
async fn dependencies(_engine: &mut PluginEngine, repo: LocalGitRepo) -> Result<NpmDependencies> {
	tracing::info!("running dependencies query");
	let path = &repo.path;

	let npm_version = get_npm_version().map_err(|e| {
		tracing::error!("{}", e);
		Error::UnspecifiedQueryState
	})?;
	check_version(&npm_version).map_err(|e| {
		tracing::error!("{}", e);
		Error::UnspecifiedQueryState
	})?;

	let npm_depend = NpmDependencies::resolve(path, npm_version.to_string());

	tracing::info!("completed dependencies query");
	npm_depend
}

#[derive(Clone, Debug)]
struct DependenciesPlugin;

impl Plugin for DependenciesPlugin {
	const PUBLISHER: &'static str = "mitre";

	const NAME: &'static str = "npm";

	fn set_config(&self, _config: Value) -> std::result::Result<(), ConfigError> {
		match get_npm_version() {
			Err(_) => Err(ConfigError::MissingProgram {
				program_name: "npm".to_owned().into_boxed_str(),
			}),
			Ok(_) => Ok(()),
		}
	}

	fn default_policy_expr(&self) -> hipcheck_sdk::prelude::Result<String> {
		Ok("".to_owned())
	}

	fn explain_default_query(&self) -> hipcheck_sdk::prelude::Result<Option<String>> {
		Ok(None)
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
	PluginServer::register(DependenciesPlugin {}, args.log_level)
		.listen_local(args.port)
		.await
}
