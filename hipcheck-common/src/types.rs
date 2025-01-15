// SPDX-License-Identifier: Apache-2.0

use crate::{
	error::Error,
	proto::{Query as PluginQuery, QueryState},
};

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

		let mut keys = Vec::with_capacity(value.key.len());
		for x in value.key.into_iter() {
			let value = serde_json::from_str(x.as_str()).map_err(Error::InvalidJsonInQueryKey)?;
			keys.push(value);
		}

		let mut outputs = Vec::with_capacity(value.output.len());
		for x in value.output.into_iter() {
			let value =
				serde_json::from_str(x.as_str()).map_err(Error::InvalidJsonInQueryOutput)?;
			outputs.push(value);
		}

		Ok(Query {
			id: value.id as usize,
			direction,
			publisher: value.publisher_name,
			plugin: value.plugin_name,
			query: value.query_name,
			key: keys,
			output: outputs,
			concerns: value.concern,
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
				serde_json::to_string(&output).map_err(Error::InvalidJsonInQueryKey)?;
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
		})
	}
}
