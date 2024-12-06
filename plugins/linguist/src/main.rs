// SPDX-License-Identifier: Apache-2.0

//! Plugin for determining if a particular path is a source file

mod fs;
mod linguist;

use clap::Parser;
use hipcheck_sdk::prelude::*;
use linguist::SourceFileDetector;
use serde::Deserialize;
use std::{path::PathBuf, result::Result as StdResult, sync::OnceLock};

#[derive(Deserialize)]
struct Config {
	langs_file: Option<PathBuf>,
}

static DETECTOR: OnceLock<SourceFileDetector> = OnceLock::new();

#[query(default)]
async fn is_likely_source_file(_engine: &mut PluginEngine, value: PathBuf) -> Result<bool> {
	let Some(sfd) = DETECTOR.get() else {
		return Err(Error::UnspecifiedQueryState);
	};
	Ok(sfd.is_likely_source_file(value))
}

#[derive(Clone, Debug)]
struct LinguistPlugin;

impl Plugin for LinguistPlugin {
	const PUBLISHER: &'static str = "mitre";

	const NAME: &'static str = "linguist";

	fn set_config(&self, config: Value) -> StdResult<(), ConfigError> {
		let conf: Config =
			serde_json::from_value(config).map_err(|e| ConfigError::Unspecified {
				message: e.to_string(),
			})?;
		let sfd = match conf.langs_file {
			Some(p) => SourceFileDetector::load(p).map_err(|e| ConfigError::Unspecified {
				message: e.to_string(),
			})?,
			None => {
				return Err(ConfigError::MissingRequiredConfig {
					field_name: "langs-file".to_owned(),
					field_type: "string".to_owned(),
					possible_values: vec![],
				});
			}
		};
		DETECTOR.set(sfd).map_err(|_e| ConfigError::Unspecified {
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

#[derive(Parser, Debug)]
struct Args {
	#[arg(long)]
	port: u16,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
	let args = Args::try_parse().unwrap();
	PluginServer::register(LinguistPlugin {})
		.listen(args.port)
		.await
}

#[cfg(test)]
mod test {
	use super::*;
	use pathbuf::pathbuf;

	fn source_file_detector() -> SourceFileDetector {
		SourceFileDetector::new(vec![".c"])
	}

	#[tokio::test]
	async fn test_is_likely_source_file() {
		DETECTOR.set(source_file_detector()).unwrap();
		let mut engine = PluginEngine::mock(MockResponses::new());

		let source_path = pathbuf!["source.c"];
		let not_source_path = pathbuf!["source.txt"];

		let res = is_likely_source_file(&mut engine, source_path)
			.await
			.unwrap();
		assert!(res);

		let res = is_likely_source_file(&mut engine, not_source_path)
			.await
			.unwrap();
		assert!(!res);
	}
}
