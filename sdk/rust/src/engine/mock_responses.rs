// SPDX-License-Identifier: Apache-2.0

use crate::{error::Result, target::QueryTarget};
use serde_json::Value as JsonValue;
use std::collections::HashMap;

/// A map of query endpoints to mock return values.
///
/// When using the `mock_engine` feature, calling `PluginEngine::query()` will cause this
/// structure to be referenced instead of trying to communicate with Hipcheck core. Allows
/// constructing a `PluginEngine` with which to write unit tests.
#[derive(Default, Debug)]
pub struct MockResponses(pub(crate) HashMap<(QueryTarget, JsonValue), Result<JsonValue>>);

impl MockResponses {
	pub fn new() -> Self {
		Self(HashMap::new())
	}
}

impl MockResponses {
	#[cfg(feature = "mock_engine")]
	pub fn insert<T, V, W>(
		&mut self,
		query_target: T,
		query_value: V,
		query_response: Result<W>,
	) -> Result<()>
	where
		T: TryInto<QueryTarget, Error: Into<crate::error::Error>>,
		V: serde::Serialize,
		W: serde::Serialize,
	{
		let query_target: QueryTarget = query_target.try_into().map_err(|e| e.into())?;
		let query_value: JsonValue = serde_json::to_value(query_value)
			.map_err(|source| crate::error::Error::InvalidJsonInQueryKey(Box::new(source)))?;
		let query_response = match query_response {
			Ok(v) => serde_json::to_value(v)
				.map_err(|source| crate::error::Error::InvalidJsonInQueryKey(Box::new(source))),
			Err(e) => Err(e),
		};
		self.0.insert((query_target, query_value), query_response);
		Ok(())
	}
}
