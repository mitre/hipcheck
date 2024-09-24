// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use hipcheck_sdk::prelude::*;
use sha2::{Digest, Sha256};

static SHA256_KEY_SCHEMA: &str = include_str!("../schema/query_schema_sha256.json");
static SHA256_OUTPUT_SCHEMA: &str = include_str!("../schema/query_schema_sha256.json");

/// calculate sha256 of provided content
fn sha256(content: &[u8]) -> Vec<u8> {
	let mut hasher = Sha256::new();
	hasher.update(content);
	hasher.finalize().to_vec()
}

/// This plugin takes in a Value::Array(Vec<Value::Number>) and calculates its sha256
#[derive(Clone, Debug)]
struct Sha256Plugin;

#[async_trait]
impl Query for Sha256Plugin {
	fn input_schema(&self) -> JsonSchema {
		from_str(SHA256_KEY_SCHEMA).unwrap()
	}

	fn output_schema(&self) -> JsonSchema {
		from_str(SHA256_OUTPUT_SCHEMA).unwrap()
	}

	async fn run(
		&self,
		_engine: &mut PluginEngine,
		input: Value,
	) -> hipcheck_sdk::error::Result<Value> {
		let Value::Array(data) = &input else {
			return Err(Error::UnexpectedPluginQueryInputFormat);
		};

		let data = data
			.iter()
			.map(|elem| elem.as_u64().map(|num| num as u8))
			.collect::<Option<Vec<_>>>()
			.ok_or_else(|| Error::UnexpectedPluginQueryInputFormat)?;

		let hash = sha256(&data);
		// convert to Value
		let hash = hash.iter().map(|x| Value::Number((*x).into())).collect();
		Ok(hash)
	}
}

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

	fn queries(&self) -> impl Iterator<Item = NamedQuery> {
		vec![NamedQuery {
			name: "sha256",
			inner: Box::new(Sha256Plugin),
		}]
		.into_iter()
	}
}

#[derive(Parser, Debug)]
struct Args {
	#[arg(long)]
	port: u16,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
	let args = Args::try_parse().unwrap();
	PluginServer::register(Sha256Plugin).listen(args.port).await
}
