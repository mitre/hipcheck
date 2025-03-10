// SPDX-License-Identifier: Apache-2.0

mod binary_detector;
mod error;
mod util;

use crate::binary_detector::{detect_binary_files, BinaryFileDetector};
use clap::Parser;
use hipcheck_sdk::{
	macros::PluginConfig,
	prelude::*,
	types::{LocalGitRepo, Target},
	LogLevel, PluginConfig,
};
use pathbuf::pathbuf;
use std::{ops::Not, path::PathBuf, result::Result as StdResult, sync::OnceLock};
pub static DETECTOR: OnceLock<BinaryFileDetector> = OnceLock::new();

#[derive(PluginConfig, Debug)]
struct Config {
	binary_file: PathBuf,
	binary_file_threshold: Option<u64>,
}

#[query]
async fn files(_engine: &mut PluginEngine, value: LocalGitRepo) -> Result<Vec<PathBuf>> {
	tracing::info!("running files query");
	let bfd = DETECTOR.get().ok_or(Error::UnspecifiedQueryState)?;
	let repo = pathbuf![&value.path];
	let out: Vec<PathBuf> = detect_binary_files(&repo)
		.map_err(|_| Error::UnspecifiedQueryState)?
		.into_iter()
		.filter(|f| bfd.is_likely_binary_file(f))
		.collect();
	tracing::info!("completed files query");
	Ok(out)
}

#[query(default)]
async fn binary(engine: &mut PluginEngine, value: Target) -> Result<usize> {
	tracing::info!("running binary query");
	let paths = files(engine, value.local).await?;
	paths.iter().for_each(|f| {
		engine.record_concern(format!("Found binary file at '{}'", f.to_string_lossy()))
	});
	tracing::info!("completed binary query");
	Ok(paths.len())
}

#[derive(Clone, Debug, Default)]
struct BinaryPlugin {
	policy_conf: OnceLock<Option<u64>>,
}

impl Plugin for BinaryPlugin {
	const PUBLISHER: &'static str = "mitre";
	const NAME: &'static str = "binary";

	fn set_config(&self, config: Value) -> StdResult<(), ConfigError> {
		// Deserialize and validate the config struct
		let conf = Config::deserialize(&config)?;

		// Store the policy conf to be accessed only in the `default_policy_expr()` impl
		self.policy_conf
			.set(conf.binary_file_threshold)
			.map_err(|_| ConfigError::InternalError {
				message: "plugin was already configured".to_string(),
			})?;

		if std::fs::exists(&conf.binary_file)
			.map_err(|e| ConfigError::InternalError {
				message: format!("failed to check file existence: {e}"),
			})?
			.not()
		{
			return Err(ConfigError::FileNotFound {
				file_path: format!("{}", conf.binary_file.display()),
			});
		}

		// Use the langs file to create a SourceFileDetector and init the salsa db
		let bfd =
			BinaryFileDetector::load(&conf.binary_file).map_err(|e| ConfigError::ParseError {
				source: format!(
					"binary file type definitions at {}",
					conf.binary_file.display()
				),
				message: e.to_string_pretty_multiline(),
			})?;

		// Make the salsa db globally accessible
		DETECTOR.set(bfd).map_err(|_e| ConfigError::InternalError {
			message: "config was already set".to_owned(),
		})
	}

	fn default_policy_expr(&self) -> Result<String> {
		match self.policy_conf.get() {
			None => Err(Error::UnspecifiedQueryState),
			Some(policy_conf) => Ok(format!("(lte $ {})", policy_conf.unwrap_or(0))),
		}
	}

	fn explain_default_query(&self) -> Result<Option<String>> {
		Ok(Some(
			"the number of detected binary files in a repo".to_owned(),
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
	PluginServer::register(BinaryPlugin::default(), args.log_level)
		.listen_local(args.port)
		.await
}
