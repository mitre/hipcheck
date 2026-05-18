// SPDX-License-Identifier: Apache-2.0

use crate::{
	engine::PluginEngine,
	error::{Error, Result},
	target::QueryTarget,
};
use serde_json::Value as JsonValue;

/// Used for building a up a `Vec` of keys to send to specific hipcheck plugin
pub struct QueryBuilder<'engine> {
	keys: Vec<JsonValue>,
	target: QueryTarget,
	plugin_engine: &'engine mut PluginEngine,
}

impl<'engine> QueryBuilder<'engine> {
	/// Create a new `QueryBuilder` to dynamically add keys to send to `target` plugin
	pub(crate) fn new<T>(
		plugin_engine: &'engine mut PluginEngine,
		target: T,
	) -> Result<QueryBuilder<'engine>>
	where
		T: TryInto<QueryTarget, Error: Into<Error>>,
	{
		let target: QueryTarget = target.try_into().map_err(|e| e.into())?;
		Ok(Self {
			plugin_engine,
			target,
			keys: vec![],
		})
	}

	/// Add a key to the internal list of keys to be sent to `target`
	///
	/// Returns the index `key` was inserted was inserted to
	pub fn query(&mut self, key: JsonValue) -> usize {
		let len = self.keys.len();
		self.keys.push(key);
		len
	}

	/// Send all of the provided keys to `target` plugin endpont and wait for query results
	pub async fn send(self) -> Result<Vec<JsonValue>> {
		self.plugin_engine.batch_query(self.target, self.keys).await
	}
}
