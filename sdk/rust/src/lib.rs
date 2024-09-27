// SPDX-License-Identifier: Apache-2.0

use crate::error::Error;
use crate::error::Result;
use error::ConfigError;
use plugin_engine::PluginEngine;
use schemars::schema::SchemaObject as JsonSchema;
use serde_json::Value as JsonValue;
use std::result::Result as StdResult;
use std::str::FromStr;

#[cfg(feature = "macros")]
extern crate hipcheck_sdk_macros;

mod proto {
	include!(concat!(env!("OUT_DIR"), "/hipcheck.v1.rs"));
}

pub mod error;
mod mock;
pub mod plugin_engine;
pub mod plugin_server;

/// A utility module, users can simply write `use hipcheck_sdk::prelude::*` to import everything
/// they need to write a plugin
pub mod prelude {
	pub use crate::deps::*;
	pub use crate::error::{ConfigError, Error, Result};
	pub use crate::plugin_engine::PluginEngine;
	pub use crate::plugin_server::{PluginServer, QueryResult};
	pub use crate::{DynQuery, NamedQuery, Plugin, Query, QuerySchema, QueryTarget};
	// Re-export macros
	#[cfg(feature = "macros")]
	pub use hipcheck_sdk_macros::{queries, query};

	#[cfg(feature = "mock_engine")]
	pub use crate::mock::MockResponses;
}

/// re-export of user-facing third-party dependencies
pub mod deps {
	pub use schemars::{schema::SchemaObject as JsonSchema, schema_for};
	pub use serde_json::{from_str, from_value, to_value, Value};
	pub use tonic::async_trait;
}

/// The target of a Hipcheck query. The `publisher` and `plugin` fields are necessary to identify a
/// plugin process. Plugins may define one or more query endpoints, and may include an unnamed
/// endpoint as the "default", hence why the `query` field is of type Option. QueryTarget
/// implements `FromStr`, taking strings of the format `"publisher/plugin[/query]"` where the
/// bracketed substring is optional.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
			_ => Err(Error::InvalidQueryTargetFormat),
		}
	}
}

impl TryInto<QueryTarget> for &str {
	type Error = Error;
	fn try_into(self) -> StdResult<QueryTarget, Self::Error> {
		QueryTarget::from_str(self)
	}
}

/// Encapsulates the signature of a particular `NamedQuery`. Instances of this type are usually
/// created by the default implementation of `Plugin::schemas()` and would not need to be created
/// by hand unless you are doing something very unorthodox.
pub struct QuerySchema {
	/// The name of the query being described.
	query_name: &'static str,

	/// The query's input schema as a `schemars::schema::SchemaObject`.
	input_schema: JsonSchema,

	/// The query's output schema as a `schemars::schema::SchemaObject`.
	output_schema: JsonSchema,
}

/// A `Query` trait object.
pub type DynQuery = Box<dyn Query>;

/// Since the `Query` trait needs to be made into a trait object, we can't use a static associated
/// string to store the query's name in the trait itself. This object wraps a `Query` trait object
/// and allows us to associate a name with it.
pub struct NamedQuery {
	/// The name of the query.
	pub name: &'static str,

	/// The `Query` trait object.
	pub inner: DynQuery,
}

impl NamedQuery {
	/// Returns whether the current query is the plugin's default query, determined by whether the
	/// query name is empty.
	fn is_default(&self) -> bool {
		self.name.is_empty()
	}
}

/// Defines a single query endpoint for the plugin.
#[tonic::async_trait]
pub trait Query: Send {
	/// Get the input schema for the query as a `schemars::schema::SchemaObject`.
	fn input_schema(&self) -> JsonSchema;

	/// Get the output schema for the query as a `schemars::schema::SchemaObject`.
	fn output_schema(&self) -> JsonSchema;

	/// Run the query endpoint logic on `input`, returning a JSONified return value on success.
	/// The `PluginEngine` reference allows the endpoint to query other Hipcheck plugins by
	/// calling `engine::query()`.
	async fn run(&self, engine: &mut PluginEngine, input: JsonValue) -> Result<JsonValue>;
}

/// The core trait that a plugin author must implement to write a plugin with the Hipcheck SDK.
/// Declares basic information about the plugin and its query endpoints, and accepts a
/// configuration map from Hipcheck core.
pub trait Plugin: Send + Sync + 'static {
	/// The name of the plugin publisher.
	const PUBLISHER: &'static str;

	/// The name of the plugin.
	const NAME: &'static str;

	/// Handle setting configuration. The `config` parameter is a JSON object of `String, String`
	/// pairs.
	fn set_config(&self, config: JsonValue) -> StdResult<(), ConfigError>;

	/// Get the plugin's default policy expression. This will only ever be called after
	/// `Plugin::set_config()`. For more information on policy expression syntax, see the Hipcheck
	/// website.
	fn default_policy_expr(&self) -> Result<String>;

	/// Get an unstructured description of what is returned by the plugin's default query.
	fn explain_default_query(&self) -> Result<Option<String>>;

	/// Get all the queries supported by the plugin. Each query endpoint in a plugin will have its
	/// own `trait Query` implementation. This function should return an iterator containing one
	/// `NamedQuery` instance ofr each `trait Query` implementation defined by the plugin author.
	fn queries(&self) -> impl Iterator<Item = NamedQuery>;

	/// Get the plugin's default query, if it has one. The default query is a `NamedQuery` with an
	/// empty `name` string. In most cases users should not need to override the default
	/// implementation.
	fn default_query(&self) -> Option<DynQuery> {
		self.queries()
			.find_map(|named| named.is_default().then_some(named.inner))
	}

	/// Get all schemas for queries provided by the plugin. In most cases users should not need to
	/// override the default implementation.
	fn schemas(&self) -> impl Iterator<Item = QuerySchema> {
		self.queries().map(|query| QuerySchema {
			query_name: query.name,
			input_schema: query.inner.input_schema(),
			output_schema: query.inner.output_schema(),
		})
	}
}
