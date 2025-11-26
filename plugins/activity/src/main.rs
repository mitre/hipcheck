// SPDX-License-Identifier: Apache-2.0

//! Plugin for querying how long it has been since a commit was last made to a repo

use clap::Parser;
use hipcheck_sdk::LogLevel;
use hipcheck_sdk::{prelude::*, types::Target};
use jiff::Timestamp;
use serde::Deserialize;
use std::{result::Result as StdResult, sync::OnceLock};

#[derive(Deserialize)]
struct Config {
	weeks: Option<u16>,
}

static CONFIG: OnceLock<Config> = OnceLock::new();

/// Returns the span of time since the most recent commit to a Git repo as `jiff:Span` displayed as a String
/// (Which means that anything expecting a `Span` must parse the output of this query appropriately)
#[query]
async fn activity(engine: &mut PluginEngine, target: Target) -> Result<String> {
	tracing::info!("running activity query");

	let repo = target.local;

	// Get today's date
	let today = Timestamp::now();

	// Get the date of the most recent commit.
	let value = engine
		.query("mitre/git/last_commit_date", repo)
		.await
		.map_err(|e| {
			tracing::error!("failed to get last commit date for activity metric: {}", e);
			Error::UnspecifiedQueryState
		})?;

	let Value::String(date_string) = value else {
		return Err(Error::UnexpectedPluginQueryInputFormat);
	};
	let last_commit_date: Timestamp = date_string.parse().map_err(|e| {
		tracing::error!("{}", e);
		Error::UnspecifiedQueryState
	})?;

	// Get the time between the most recent commit and today.
	let time_since_last_commit = today.since(last_commit_date).map_err(|e| {
		tracing::error!("{}", e);
		Error::UnspecifiedQueryState
	})?;

	tracing::info!("completed activity query");
	Ok(time_since_last_commit.to_string())
}

#[derive(Clone, Debug)]
struct ActivityPlugin;

impl Plugin for ActivityPlugin {
	const PUBLISHER: &'static str = "mitre";

	const NAME: &'static str = "activity";

	fn set_config(&self, config: Value) -> StdResult<(), ConfigError> {
		let conf =
			serde_json::from_value::<Config>(config).map_err(|e| ConfigError::Unspecified {
				message: e.to_string().into_boxed_str(),
			})?;
		CONFIG.set(conf).map_err(|_e| ConfigError::InternalError {
			message: "config was already set".to_owned().into_boxed_str(),
		})
	}

	fn default_policy_expr(&self) -> Result<String> {
		let Some(conf) = CONFIG.get() else {
			tracing::error!("tried to access config before set by Hipcheck core!");
			return Err(Error::UnspecifiedQueryState);
		};

		Ok(format!("(lte $ P{}w)", conf.weeks.unwrap_or(71)))
	}

	fn explain_default_query(&self) -> Result<Option<String>> {
		Ok(Some(
			"span of time that has elapsed since last activity in repo".to_string(),
		))
	}

	queries! {
		#[default] activity
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
	PluginServer::register(ActivityPlugin {}, args.log_level)
		.listen_local(args.port)
		.await
}

#[cfg(test)]
mod test {
	use super::*;

	use hipcheck_sdk::types::LocalGitRepo;
	use jiff::{Span, SpanCompare, SpanRound, Unit};
	use std::{cmp::Ordering, result::Result as StdResult};

	fn repo() -> LocalGitRepo {
		LocalGitRepo {
			path: "/home/users/me/.cache/hipcheck/clones/github/expressjs/express/".to_string(),
			git_ref: "main".to_string(),
		}
	}

	fn mock_responses() -> StdResult<MockResponses, Error> {
		let repo = repo();
		let output = "2024-06-19T19:22:45Z".to_string();

		// when calling into query, the input repo gets passed to `last_commit_date`, lets assume it returns the datetime `output`
		let mut mock_responses = MockResponses::new();
		mock_responses.insert("mitre/git/last_commit_date", repo, Ok(output))?;
		Ok(mock_responses)
	}

	#[tokio::test]
	async fn test_activity() {
		let repo = repo();
		let target = Target {
			specifier: "express".to_string(),
			local: repo,
			remote: None,
			package: None,
		};

		let mut engine = PluginEngine::mock(mock_responses().unwrap());
		let output = activity(&mut engine, target).await.unwrap();
		let span: Span = output.parse().unwrap();
		let result = span
			.round(SpanRound::new().days_are_24_hours().smallest(Unit::Day))
			.unwrap();

		let today = Timestamp::now();
		let last_commit: Timestamp = "2024-06-19T19:22:45Z".parse().unwrap();
		let expected = today
			.since(last_commit)
			.unwrap()
			.round(SpanRound::new().days_are_24_hours().smallest(Unit::Day))
			.unwrap();

		let expected = SpanCompare::from(expected).days_are_24_hours();
		assert_eq!(result.compare(expected).unwrap(), Ordering::Equal);
	}
}
