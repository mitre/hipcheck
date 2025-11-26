// SPDX-License-Identifier: Apache-2.0

//! Plugin for determining if a particular path is a source file

mod error;
mod linguist;
mod util;

use clap::Parser;
use hipcheck_sdk::{LogLevel, prelude::*};
use linguist::SourceFileDetector;
use serde::Deserialize;
use std::{path::PathBuf, result::Result as StdResult, sync::OnceLock};

#[derive(Deserialize)]
struct Config {
	#[serde(rename = "langs-file")]
	langs_file: Option<PathBuf>,
}

static DETECTOR: OnceLock<SourceFileDetector> = OnceLock::new();

#[query]
async fn is_likely_source_file(_engine: &mut PluginEngine, value: PathBuf) -> Result<bool> {
	tracing::debug!("running is_likely_source_file query");
	let Some(sfd) = DETECTOR.get() else {
		return Err(Error::UnspecifiedQueryState);
	};
	let is_source = sfd.is_likely_source_file(value);

	tracing::debug!("completed is_likely_source_file query");
	Ok(is_source)
}

#[derive(Clone, Debug)]
struct LinguistPlugin;

impl Plugin for LinguistPlugin {
	const PUBLISHER: &'static str = "mitre";

	const NAME: &'static str = "linguist";

	fn set_config(&self, config: Value) -> StdResult<(), ConfigError> {
		let conf: Config =
			serde_json::from_value(config).map_err(|e| ConfigError::Unspecified {
				message: e.to_string().into_boxed_str(),
			})?;
		let sfd = match conf.langs_file {
			Some(p) => SourceFileDetector::load(&p).map_err(|e| ConfigError::ParseError {
				source: format!("Language definitions file at {}", p.display()).into_boxed_str(),
				message: e.to_string_pretty_multiline().into_boxed_str(),
			})?,
			None => {
				return Err(ConfigError::MissingRequiredConfig {
					field_name: "langs-file".to_owned().into_boxed_str(),
					field_type: "string".to_owned().into_boxed_str(),
					possible_values: vec![],
				});
			}
		};
		DETECTOR.set(sfd).map_err(|_e| ConfigError::InternalError {
			message: "config was already set".to_owned().into_boxed_str(),
		})
	}

	fn default_policy_expr(&self) -> Result<String> {
		Ok("".to_owned())
	}

	fn explain_default_query(&self) -> Result<Option<String>> {
		Ok(None)
	}

	queries! {
		#[default] is_likely_source_file,
	}
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
	PluginServer::register(LinguistPlugin {}, args.log_level)
		.listen_local(args.port)
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
