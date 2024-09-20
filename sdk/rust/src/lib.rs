// SPDX-License-Identifier: Apache-2.0

use crate::error::Error;
use crate::error::Result;
use error::ConfigError;
use plugin_engine::PluginEngine;
use schemars::schema::SchemaObject as JsonSchema;
use serde_json::Value as JsonValue;
use std::result::Result as StdResult;
use std::str::FromStr;

mod proto {
	include!(concat!(env!("OUT_DIR"), "/hipcheck.v1.rs"));
}

pub mod error;
pub mod plugin_engine;
pub mod plugin_server;

// utility module, so users can write `use hipcheck_sdk::prelude::*` and have everything they need to write a plugin
pub mod prelude {
	pub use crate::deps::*;
	pub use crate::error::{ConfigError, Error, Result};
	pub use crate::plugin_engine::PluginEngine;
	pub use crate::plugin_server::{PluginServer, QueryResult};
	pub use crate::{DynQuery, NamedQuery, Plugin, Query, QuerySchema, QueryTarget};
}

// re-export of user facing third party dependencies
pub mod deps {
	pub use schemars::schema::SchemaObject as JsonSchema;
	pub use serde_json::{from_str, Value};
	pub use tonic::async_trait;
}

#[derive(Debug, Clone)]
pub struct QueryTarget {
	pub publisher: String,
	pub plugin: String,
	pub query: Option<String>,
}

impl FromStr for QueryTarget {
	type Err = Error;

	fn from_str(s: &str) -> StdResult<Self, Self::Err> {
		let parts: Vec<&str> = s.split('/').collect();
		match parts.as_slice() {
			[publisher, plugin, query] => Ok(Self {
				publisher: publisher.to_string(),
				plugin: plugin.to_string(),
				query: Some(query.to_string()),
			}),
			[publisher, plugin] => Ok(Self {
				publisher: publisher.to_string(),
				plugin: plugin.to_string(),
				query: None,
			}),
			_ => Err(Error::InvalidQueryTarget),
		}
	}
}

impl TryInto<QueryTarget> for &str {
	type Error = Error;
	fn try_into(self) -> StdResult<QueryTarget, Self::Error> {
		QueryTarget::from_str(self)
	}
}

pub struct QuerySchema {
	/// The name of the query being described.
	query_name: &'static str,

	/// The query's input schema.
	input_schema: JsonSchema,

	/// The query's output schema.
	output_schema: JsonSchema,
}

/// Query trait object.
pub type DynQuery = Box<dyn Query>;

pub struct NamedQuery {
	/// The name of the query.
	pub name: &'static str,

	/// The query object.
	pub inner: DynQuery,
}

impl NamedQuery {
	/// Is the current query the default query?
	fn is_default(&self) -> bool {
		self.name.is_empty()
	}
}

/// Defines a single query for the plugin.
#[tonic::async_trait]
pub trait Query: Send {
	/// Get the input schema for the query.
	fn input_schema(&self) -> JsonSchema;

	/// Get the output schema for the query.
	fn output_schema(&self) -> JsonSchema;

	/// Run the plugin, optionally making queries to other plugins.
	async fn run(&self, engine: &mut PluginEngine, input: JsonValue) -> Result<JsonValue>;
}

pub trait Plugin: Send + Sync + 'static {
	/// The name of the publisher of the pl∂ugin.
	const PUBLISHER: &'static str;

	/// The name of the plugin.
	const NAME: &'static str;

	/// Handles setting configuration.
	fn set_config(&self, config: JsonValue) -> StdResult<(), ConfigError>;

	/// Gets the plugin's default policy expression.
	fn default_policy_expr(&self) -> Result<String>;

	/// Gets a description of what is returned by the plugin's default query.
	fn explain_default_query(&self) -> Result<Option<String>>;

	/// Get all the queries supported by the plugin.
	fn queries(&self) -> impl Iterator<Item = NamedQuery>;

	/// Get the plugin's default query, if it has one.
	fn default_query(&self) -> Option<DynQuery> {
		self.queries()
			.find_map(|named| named.is_default().then_some(named.inner))
	}

	/// Get all schemas for queries provided by the plugin.
	fn schemas(&self) -> impl Iterator<Item = QuerySchema> {
		self.queries().map(|query| QuerySchema {
			query_name: query.name,
			input_schema: query.inner.input_schema(),
			output_schema: query.inner.output_schema(),
		})
	}
}
