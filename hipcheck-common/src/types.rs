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
	pub key: serde_json::Value,
	pub output: serde_json::Value,
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
			QueryState::Submit => Ok(QueryDirection::Request),
			QueryState::ReplyInProgress => Err(Error::UnexpectedReplyInProgress),
			QueryState::ReplyComplete => Ok(QueryDirection::Response),
		}
	}
}

impl From<QueryDirection> for QueryState {
	fn from(value: QueryDirection) -> Self {
		match value {
			QueryDirection::Request => QueryState::Submit,
			QueryDirection::Response => QueryState::ReplyComplete,
		}
	}
}

impl TryFrom<PluginQuery> for Query {
	type Error = Error;

	fn try_from(value: PluginQuery) -> Result<Query, Self::Error> {
		Ok(Query {
			id: value.id as usize,
			direction: QueryDirection::try_from(value.state())?,
			publisher: value.publisher_name,
			plugin: value.plugin_name,
			query: value.query_name,
			key: serde_json::from_str(value.key.as_str()).map_err(Error::InvalidJsonInQueryKey)?,
			output: serde_json::from_str(value.output.as_str())
				.map_err(Error::InvalidJsonInQueryOutput)?,
			concerns: value.concern,
		})
	}
}

impl TryFrom<Query> for PluginQuery {
	type Error = Error;

	fn try_from(value: Query) -> Result<PluginQuery, Self::Error> {
		let state: QueryState = value.direction.into();
		let key = serde_json::to_string(&value.key).map_err(Error::InvalidJsonInQueryKey)?;
		let output =
			serde_json::to_string(&value.output).map_err(Error::InvalidJsonInQueryOutput)?;

		Ok(PluginQuery {
			id: value.id as i32,
			state: state as i32,
			publisher_name: value.publisher,
			plugin_name: value.plugin,
			query_name: value.query,
			key,
			output,
			concern: value.concerns,
		})
	}
}
