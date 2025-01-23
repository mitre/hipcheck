// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use hipcheck_sdk::{prelude::*, types::Target};
use serde_json::Value;
use std::result::Result as StdResult;

/// Returns whether the target's remote repo uses Google's OSS fuzzing
#[query(default)]
async fn fuzz(engine: &mut PluginEngine, key: Target) -> Result<Value> {
	if let Some(remote) = &key.remote {
		engine.query("mitre/github", remote.clone()).await
	} else {
		Err(Error::UnexpectedPluginQueryInputFormat)
	}
}

#[derive(Parser, Debug)]
struct Args {
	#[arg(long)]
	port: u16,
}

#[derive(Clone, Debug)]
struct FuzzAnalysisPlugin {}

impl Plugin for FuzzAnalysisPlugin {
	const PUBLISHER: &'static str = "mitre";
	const NAME: &'static str = "fuzz";

	fn set_config(&self, _config: Value) -> StdResult<(), ConfigError> {
		Ok(())
	}

	fn default_policy_expr(&self) -> Result<String> {
		Ok("(eq $ #t)".to_owned())
	}

	fn explain_default_query(&self) -> Result<Option<String>> {
		Ok(Some("'Does the target repo do fuzzing?'".to_owned()))
	}

	queries! {}
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
	let args = Args::try_parse().unwrap();
	PluginServer::register(FuzzAnalysisPlugin {})
		.listen(args.port)
		.await
}

#[cfg(test)]
mod test {
	use super::*;
	use hipcheck_sdk::types::{KnownRemote, LocalGitRepo, RemoteGitRepo};

	fn target() -> Target {
		let local = LocalGitRepo {
			path: "/home/users/me/.cache/hipcheck/clones/github/mitre/hipcheck/".to_string(),
			git_ref: "main".to_string(),
		};
		let known_remote = Some(KnownRemote::GitHub {
			owner: "mitre".to_owned(),
			repo: "hipcheck".to_owned(),
		});
		let remote = Some(RemoteGitRepo {
			url: "https://github.com/mitre/hipcheck".parse().unwrap(),
			known_remote,
		});
		Target {
			specifier: "hipcheck".to_owned(),
			local,
			remote,
			package: None,
		}
	}

	fn mock_responses() -> StdResult<MockResponses, Error> {
		let target = target();
		let known_remote = target.remote.as_ref().unwrap().clone();
		let output = true;
		let mut mock_reponses = MockResponses::new();
		mock_reponses.insert("mitre/github", known_remote, Ok(output))?;
		Ok(mock_reponses)
	}

	#[tokio::test]
	async fn test_fuzz() {
		let target = target();
		let mut engine = PluginEngine::mock(mock_responses().unwrap());
		let output = fuzz(&mut engine, target).await.unwrap();
		let result: bool = serde_json::from_value(output).unwrap();
		let expected = true;

		assert_eq!(result, expected);
	}
}
