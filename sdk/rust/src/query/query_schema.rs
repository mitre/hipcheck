// SPDX-License-Identifier: Apache-2.0

use schemars::Schema as JsonSchema;

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
