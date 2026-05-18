use crate::{engine::PluginEngine, error::Result};
use schemars::Schema as JsonSchema;
use serde_json::Value as JsonValue;

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

/// Describes the signature of a particular `NamedQuery`.
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

impl QuerySchema {
	pub fn new(
		query_name: &'static str,
		input_schema: JsonSchema,
		output_schema: JsonSchema,
	) -> Self {
		QuerySchema {
			query_name,
			input_schema,
			output_schema,
		}
	}

	pub fn query_name(&self) -> &'static str {
		self.query_name
	}

	pub fn input_schema(&self) -> &JsonSchema {
		&self.input_schema
	}

	pub fn output_schema(&self) -> &JsonSchema {
		&self.output_schema
	}
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
	pub(crate) fn is_default(&self) -> bool {
		self.name.is_empty()
	}
}
