// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use hipcheck_sdk::{prelude::*, LogLevel};
use sha2::{Digest, Sha256};

#[query(default)]
async fn query_sha256(_engine: &mut PluginEngine, content: Vec<u8>) -> Result<Vec<u8>> {
	let mut hasher = Sha256::new();
	hasher.update(content.as_slice());
	Ok(hasher.finalize().to_vec())
}

/// This plugin takes in a Value::Array(Vec<Value::Number>) and calculates its sha256
#[derive(Clone, Debug)]
struct Sha256Plugin;

impl Plugin for Sha256Plugin {
	const PUBLISHER: &'static str = "dummy";

	const NAME: &'static str = "sha256";

	fn set_config(&self, _config: Value) -> std::result::Result<(), ConfigError> {
		Ok(())
	}

	fn default_policy_expr(&self) -> hipcheck_sdk::prelude::Result<String> {
		Ok("".to_owned())
	}

	fn explain_default_query(&self) -> hipcheck_sdk::prelude::Result<Option<String>> {
		Ok(Some("calculate sha256 of provided array".to_owned()))
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
	PluginServer::register(Sha256Plugin, args.log_level)
		.listen_local(args.port)
		.await
}
