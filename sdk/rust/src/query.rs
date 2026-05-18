// SPDX-License-Identifier: Apache-2.0

use crate::{engine::PluginEngine, error::Result};
use schemars::Schema as JsonSchema;
use serde_json::Value as JsonValue;

pub(crate) mod query_endpoint;
pub(crate) mod query_schema;

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

/// A `Query` trait object.
pub type DynQuery = Box<dyn Query>;
