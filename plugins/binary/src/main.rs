// SPDX-License-Identifier: Apache-2.0

mod binary_detector;
mod error;
mod fs;

use crate::binary_detector::{detect_binary_files, BinaryFileDetector};

use clap::Parser;
use hipcheck_sdk::{prelude::*, types::Target};
use pathbuf::pathbuf;
use serde::Deserialize;

use std::{path::PathBuf, result::Result as StdResult, sync::OnceLock};

pub static DETECTOR: OnceLock<BinaryFileDetector> = OnceLock::new();

#[derive(Deserialize)]
struct RawConfig {
	#[serde(rename = "binary-file")]
	binary_file: Option<PathBuf>,
	#[serde(rename = "binary-file-threshold")]
	binary_file_threshold: Option<u64>,
}

struct Config {
	binary_file: PathBuf,
	opt_threshold: Option<u64>,
}

impl TryFrom<RawConfig> for Config {
	type Error = hipcheck_sdk::error::ConfigError;
	fn try_from(value: RawConfig) -> StdResult<Config, Self::Error> {
		let Some(binary_file) = value.binary_file else {
			return Err(ConfigError::MissingRequiredConfig {
				field_name: "binary-file".to_owned(),
				field_type: "string".to_owned(),
				possible_values: vec![],
			});
		};
		let opt_threshold = value.binary_file_threshold;
		Ok(Config {
			binary_file,
			opt_threshold,
		})
	}
}

/// Returns a boolean list with one entry per possible binary file in the repo
/// A `true` entry corresponds to a binary file that matches a known file extension
#[query(default)]
async fn binary(engine: &mut PluginEngine, value: Target) -> Result<Vec<bool>> {
	// get binary detector
	let bfd = DETECTOR.get().ok_or(Error::UnspecifiedQueryState)?;
	// Get the paths of all possible binary files in the repo
	let repo = pathbuf![&value.local.path];
	let files = detect_binary_files(&repo).map_err(|_| Error::UnspecifiedQueryState)?;
	// Get the subset of those files that match a known binary file extension
	let out: Vec<PathBuf> = files
		.clone()
		.into_iter()
		.filter(|f| bfd.is_likely_binary_file(f))
		.collect();

	// Generate a boolean list of possible binary files, with true indicating a match for a known extension
	let binaries = files.iter().map(|d| out.contains(d)).collect();

	// Add each matched binary file as a concern
	out.iter().for_each(|f| {
		engine.record_concern(format!("Found binary file at '{}'", f.to_string_lossy()))
	});

	Ok(binaries)
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
		let conf: Config = serde_json::from_value::<RawConfig>(config)
			.map_err(|e| ConfigError::Unspecified {
				message: e.to_string(),
			})?
			.try_into()?;

		// Store the policy conf to be accessed only in the `default_policy_expr()` impl
		self.policy_conf
			.set(conf.opt_threshold)
			.map_err(|_| ConfigError::Unspecified {
				message: "plugin was already configured".to_string(),
			})?;

		// Use the langs file to create a SourceFileDetector and init the salsa db
		let bfd =
			BinaryFileDetector::load(conf.binary_file).map_err(|e| ConfigError::Unspecified {
				message: e.to_string(),
			})?;

		// Make the salsa db globally accessible
		DETECTOR.set(bfd).map_err(|_e| ConfigError::Unspecified {
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
			"Returns number of detected binary files in a repo".to_owned(),
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
	PluginServer::register(BinaryPlugin::default())
		.listen(args.port)
		.await
}
