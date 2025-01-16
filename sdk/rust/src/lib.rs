// SPDX-License-Identifier: Apache-2.0

#![allow(unexpected_cfgs)]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(docsrs, allow(unused_attributes))]

//! Hipcheck Plugin SDK in Rust.
//!
//! ## What is Hipcheck?
//! [Hipcheck][hipcheck] is a command line interface (CLI) tool for analyzing open source software
//! packages and source repositories to understand their software supply chain risk. It analyzes a
//! project's software development practices and detects active supply chain attacks to give you
//! both a long-term and immediate picture of the risk from using a package.
//!
//! Part of Hipcheck's value is its [plugin system][hipcheck_plugins], which allows anyone to write
//! a new data source or analysis component, or build even higher level analyses off of the results
//! of multiple other components.
//!
//! ## The Plugin SDK
//! This crate is a Rust SDK to help developers focus on writing the essential logic of their
//! Hipcheck plugins instead of worrying about session management or communication with Hipcheck
//! core. The essential steps of using this SDK are to implement the `Query` trait for each query
//! endpoint you wish to support, then implement the `Plugin` trait to tie your plugin together and
//! describe things like configuration parameters.
//!
//! For more, see our [detailed guide][web_sdk_docs] on writing plugins using this crate.
//!
//! [hipcheck]: https://hipcheck.mitre.org/
//! [hipcheck_plugins]: https://hipcheck.mitre.org/docs/guide/making-plugins/creating-a-plugin/
//! [web_sdk_docs]: https://hipcheck.mitre.org/docs/guide/making-plugins/rust-sdk/

use crate::error::{ConfigError, Error, Result};
pub use engine::PluginEngine;
use schemars::schema::SchemaObject as JsonSchema;
use serde_json::Value as JsonValue;
pub use server::PluginServer;
use std::result::Result as StdResult;
use std::str::FromStr;

#[cfg(feature = "macros")]
#[cfg_attr(docsrs, doc(cfg(feature = "macros")))]
/// Macros for simplifying `Query` and `Plugin` trait implementations
pub mod macros {
	pub use hipcheck_sdk_macros::*;
}

#[cfg(feature = "print-timings")]
mod benchmarking;

mod engine;
pub mod error;
mod server;

#[cfg(feature = "mock_engine")]
#[cfg_attr(docsrs, doc(cfg(feature = "mock_engine")))]
/// Tools for unit-testing plugin `Query` implementations
pub mod mock {
	pub use crate::engine::MockResponses;
}

/// The definitions of Hipcheck's analysis `Target` object and its sub-types for use in writing
/// query endpoints.
pub mod types;

/// A utility module containing everything needed to write a plugin, just write `use
/// hipcheck_sdk::prelude::*`.
pub mod prelude {
	pub use crate::deps::*;
	pub use crate::engine::PluginEngine;
	pub use crate::error::{ConfigError, Error, Result};
	pub use crate::server::{PluginServer, QueryResult};
	pub use crate::{DynQuery, NamedQuery, Plugin, Query, QuerySchema, QueryTarget};
	// Re-export macros
	#[cfg(feature = "macros")]
	#[cfg_attr(docsrs, doc(cfg(feature = "macros")))]
	pub use crate::macros::{queries, query};

	#[cfg(feature = "mock_engine")]
	#[cfg_attr(docsrs, doc(cfg(feature = "mock_engine")))]
	pub use crate::engine::MockResponses;
}

/// Re-export of user-facing third-party dependencies
pub mod deps {
	pub use jiff::{Span, Zoned};
	pub use schemars::{schema::SchemaObject as JsonSchema, schema_for};
	pub use serde_json::{from_str, from_value, to_value, Value};
	pub use tonic::async_trait;
}

/// Identifies the target plugin and endpoint of a Hipcheck query.
///
/// The `publisher` and `plugin` fields are necessary from Hipcheck core's perspective to identify
/// a plugin process. Plugins may define one or more query endpoints, and may include an unnamed
/// endpoint as the "default", hence why the `query` field is optional. `QueryTarget` implements
/// `FromStr` so it can be parsed from strings of the format `"publisher/plugin[/query]"`, where
/// the bracketed substring is optional.
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

/// Descrbies the signature of a particular `NamedQuery`.
///
/// Instances of this type are usually created by the default implementation of `Plugin::schemas()`
/// and would not need to be created by hand unless you are doing something very unorthodox.
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

/// Pairs a query endpoint name with a particular `Query` trait implementation.
///
/// Since the `Query` trait needs to be made into a trait object, we can't use a static associated
/// string to store the query's name in the trait itself. This object wraps a `Query` trait object
/// and allows us to associate a name with it so that when the plugin receives a query from
/// Hipcheck core, it can look up the proper behavior to invoke.
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

/// The core trait that a plugin author must implement using the Hipcheck SDK.
///
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
	/// `NamedQuery` instance for each `trait Query` implementation defined by the plugin author.
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
