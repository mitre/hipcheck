use crate::{
	error::{ConfigError, Result},
	query::{DynQuery, NamedQuery, QuerySchema},
};
use serde_json::Value as JsonValue;
use std::result::Result as StdResult;

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
	fn default_policy_expr(&self) -> Result<String> {
		Ok(String::new())
	}

	/// Get an unstructured description of what is returned by the plugin's default query.
	fn explain_default_query(&self) -> Result<Option<String>> {
		Ok(None)
	}

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
		self.queries().map(|query| {
			QuerySchema::new(
				query.name,
				query.inner.input_schema(),
				query.inner.output_schema(),
			)
		})
	}
}
