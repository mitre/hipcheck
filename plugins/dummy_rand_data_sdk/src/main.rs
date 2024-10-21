// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use hipcheck_sdk::prelude::*;
#[cfg(test)]
use std::result::Result as StdResult;

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

#[query(default = false)]
async fn rand_data(engine: &mut PluginEngine, size: u64) -> Result<u64> {
	let reduced_num = reduce(size);

	engine.record_concern("this is a test");

	let value = engine.query("dummy/sha256", vec![reduced_num]).await?;

	let Value::Array(mut sha256) = value else {
		return Err(Error::UnexpectedPluginQueryInputFormat);
	};

	let Value::Number(num) = sha256.pop().unwrap() else {
		return Err(Error::UnexpectedPluginQueryInputFormat);
	};

	engine.record_concern("this is a test2");

	num.as_u64().ok_or(Error::UnexpectedPluginQueryOutputFormat)
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
	PluginServer::register(RandDataPlugin)
		.listen(args.port)
		.await
}

#[cfg(test)]
mod test {
	use super::*;

	fn mock_responses() -> StdResult<MockResponses, Error> {
		// when calling into query 1, Value::Array(vec![1]) gets passed to `sha256`, lets assume it returns 1
		Ok(MockResponses::new().insert("dummy/sha256", vec![1], Ok(vec![1]))?)
	}

	#[tokio::test]
	async fn test_sha256() {
		let mut engine = PluginEngine::mock(mock_responses().unwrap());
		let output = rand_data(&mut engine, 8).await;
		// 8 % 7 = 1
		assert!(matches!(output, Ok(1)));
	}
}
