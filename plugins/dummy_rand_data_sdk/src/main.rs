// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use clap::Parser;
use hipcheck_sdk::{
	deps::{async_trait, from_str, JsonSchema, Value},
	error::Error,
	plugin_engine::PluginEngine,
	plugin_server::PluginServer,
	NamedQuery, Plugin, Query,
};

static GET_RAND_KEY_SCHEMA: &str = include_str!("../schema/query_schema_get_rand.json");
static GET_RAND_OUTPUT_SCHEMA: &str = include_str!("../schema/query_schema_get_rand.json");

fn reduce(input: u64) -> u64 {
	input % 7
}

/// Plugin that queries hipcheck takes a `Value::Number` as input and performs following steps:
/// - ensures input is u64
/// - % 7 of input
/// - queries `hipcheck` for sha256 of (% 7 of input)
/// - returns `Value::Number`, where Number is the first `u8` in the sha256
///
/// Goals of this plugin
/// - Verify `salsa` memoization is working (there should only ever be 7 queries made to `hipcheck`)
/// - Verify plugins are able to query `hipcheck` for additional information
#[derive(Clone, Debug)]
struct RandDataPlugin;

#[async_trait]
impl Query for RandDataPlugin {
	fn input_schema(&self) -> JsonSchema {
		from_str(GET_RAND_KEY_SCHEMA).unwrap()
	}

	fn output_schema(&self) -> JsonSchema {
		from_str(GET_RAND_OUTPUT_SCHEMA).unwrap()
	}

	async fn run(
		&self,
		engine: &mut PluginEngine,
		input: Value,
	) -> hipcheck_sdk::error::Result<Value> {
		let Value::Number(num_size) = input else {
			return Err(Error::UnexpectedPluginQueryDataFormat);
		};

		let Some(size) = num_size.as_u64() else {
			return Err(Error::UnexpectedPluginQueryDataFormat);
		};

		let reduced_num = reduce(size);

		let value = engine
			.query("dummy/sha256/sha256", vec![reduced_num])
			.await?;

		let Value::Array(mut sha256) = value else {
			return Err(Error::UnexpectedPluginQueryDataFormat);
		};

		let Value::Number(num) = sha256.pop().unwrap() else {
			return Err(Error::UnexpectedPluginQueryDataFormat);
		};

		match num.as_u64() {
			Some(val) => return Ok(Value::Number(val.into())),
			None => {
				return Err(Error::UnexpectedPluginQueryDataFormat);
			}
		}
	}
}

impl Plugin for RandDataPlugin {
	const PUBLISHER: &'static str = "dummy";
	const NAME: &'static str = "rand_data";

	fn set_config(
		&self,
		_config: Value,
	) -> std::result::Result<(), hipcheck_sdk::error::ConfigError> {
		Ok(())
	}

	fn default_policy_expr(&self) -> hipcheck_sdk::error::Result<String> {
		Ok("".to_owned())
	}

	fn explain_default_query(&self) -> hipcheck_sdk::error::Result<Option<String>> {
		Ok(Some("generate random data".to_owned()))
	}

	fn queries(&self) -> impl Iterator<Item = hipcheck_sdk::NamedQuery> {
		vec![NamedQuery {
			name: "rand_data",
			inner: Box::new(RandDataPlugin),
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
async fn main() -> Result<(), hipcheck_sdk::error::Error> {
	let args = Args::try_parse().unwrap();
	PluginServer::register(RandDataPlugin)
		.listen(args.port)
		.await
}
