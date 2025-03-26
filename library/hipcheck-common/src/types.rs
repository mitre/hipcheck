// SPDX-License-Identifier: Apache-2.0

use crate::{
	error::Error,
	proto::{Query as PluginQuery, QueryState},
};
use serde::Deserialize;
use serde_json::Value;
use std::fmt::{self, Display, Formatter};

#[derive(Debug)]
pub struct Query {
	pub id: usize,
	pub direction: QueryDirection,
	pub publisher: String,
	pub plugin: String,
	pub query: String,
	pub key: Vec<serde_json::Value>,
	pub output: Vec<serde_json::Value>,
	pub concerns: Vec<String>,
	pub error: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum QueryDirection {
	Request,
	Response,
}

impl TryFrom<QueryState> for QueryDirection {
	type Error = Error;

	fn try_from(value: QueryState) -> Result<Self, Self::Error> {
		match value {
			QueryState::Unspecified => Err(Error::UnspecifiedQueryState),
			// Cal TODO use correct error type instead of UnspecifiedQueryState
			QueryState::Error => Err(Error::UnspecifiedQueryState),
			QueryState::SubmitInProgress => Err(Error::UnexpectedRequestInProgress),
			QueryState::SubmitComplete => Ok(QueryDirection::Request),
			QueryState::ReplyInProgress => Err(Error::UnexpectedReplyInProgress),
			QueryState::ReplyComplete => Ok(QueryDirection::Response),
		}
	}
}

impl From<QueryDirection> for QueryState {
	fn from(value: QueryDirection) -> Self {
		match value {
			QueryDirection::Request => QueryState::SubmitComplete,
			QueryDirection::Response => QueryState::ReplyComplete,
		}
	}
}

impl TryFrom<PluginQuery> for Query {
	type Error = Error;

	fn try_from(value: PluginQuery) -> Result<Query, Self::Error> {
		let direction = QueryDirection::try_from(value.state())?;

		fn get_fields_rfd9(v: &PluginQuery) -> Result<(Vec<Value>, Vec<Value>), Error> {
			let mut keys = Vec::with_capacity(v.key.len());
			for x in v.key.iter() {
				let value =
					serde_json::from_str(x.as_str()).map_err(Error::InvalidJsonInQueryKey)?;
				keys.push(value);
			}

			let mut outputs = Vec::with_capacity(v.output.len());
			for x in v.output.iter() {
				let value =
					serde_json::from_str(x.as_str()).map_err(Error::InvalidJsonInQueryOutput)?;
				outputs.push(value);
			}

			Ok((keys, outputs))
		}

		fn get_fields_compat(v: &PluginQuery) -> Result<(Vec<Value>, Vec<Value>), Error> {
			let key = v.key.join("");
			let output = v.output.join("");
			let json_key = serde_json::from_str(&key).map_err(Error::InvalidJsonInQueryKey)?;
			let json_out =
				serde_json::from_str(&output).map_err(Error::InvalidJsonInQueryOutput)?;
			Ok((vec![json_key], vec![json_out]))
		}

		let rfd9_res = get_fields_rfd9(&value);

		let (keys, outputs) = if rfd9_res.is_err() && cfg!(feature = "rfd9-compat") {
			get_fields_compat(&value)?
		} else {
			rfd9_res?
		};

		Ok(Query {
			id: value.id as usize,
			direction,
			publisher: value.publisher_name,
			plugin: value.plugin_name,
			query: value.query_name,
			key: keys,
			output: outputs,
			concerns: value.concern,
			error: value.error,
		})
	}
}

impl TryFrom<Query> for PluginQuery {
	type Error = Error;

	fn try_from(value: Query) -> Result<PluginQuery, Self::Error> {
		let state: QueryState = value.direction.into();

		let mut keys = vec![];
		for key in value.key {
			let json_formatted_key =
				serde_json::to_string(&key).map_err(Error::InvalidJsonInQueryKey)?;
			keys.push(json_formatted_key);
		}

		let mut outputs = vec![];
		for output in value.output {
			let json_formatted_output =
				serde_json::to_string(&output).map_err(Error::InvalidJsonInQueryOutput)?;
			outputs.push(json_formatted_output);
		}

		Ok(PluginQuery {
			id: value.id as i32,
			state: state as i32,
			publisher_name: value.publisher,
			plugin_name: value.plugin,
			query_name: value.query,
			key: keys,
			output: outputs,
			concern: value.concerns,
			split: false,
			error: value.error,
		})
	}
}

#[derive(Debug, Deserialize, PartialEq, Clone, clap::ValueEnum)]
#[serde(rename_all = "UPPERCASE")]
pub enum LogLevel {
	Off,
	Error,
	Warn,
	Info,
	Debug,
	Trace,
}
impl LogLevel {
	pub fn level_from_str(s: &str) -> Result<LogLevel, String> {
		match s.trim().to_lowercase().as_str() {
			"off" => Ok(LogLevel::Off),
			"error" => Ok(LogLevel::Error),
			"warn" => Ok(LogLevel::Warn),
			"info" => Ok(LogLevel::Info),
			"debug" => Ok(LogLevel::Debug),
			"trace" => Ok(LogLevel::Trace),
			_ => Err(format!("Invalid log level: {}", s.trim())),
		}
	}
}

impl Display for LogLevel {
	fn fmt(&self, f: &mut Formatter) -> fmt::Result {
		match &self {
			LogLevel::Off => write!(f, "off"),
			LogLevel::Error => write!(f, "error"),
			LogLevel::Warn => write!(f, "warn"),
			LogLevel::Info => write!(f, "info"),
			LogLevel::Debug => write!(f, "debug"),
			LogLevel::Trace => write!(f, "trace"),
		}
	}
}
