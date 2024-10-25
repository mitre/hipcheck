// SPDX-License-Identifier: Apache-2.0

use crate::{JsonValue, QueryTarget, Result};
use std::collections::HashMap;

#[derive(Default, Debug)]
pub struct MockResponses(pub(crate) HashMap<(QueryTarget, JsonValue), Result<JsonValue>>);

impl MockResponses {
	pub fn new() -> Self {
		Self(HashMap::new())
	}
}

#[cfg(feature = "mock_engine")]
impl MockResponses {
	pub fn insert<T, V, W>(
		&mut self,
		query_target: T,
		query_value: V,
		query_response: Result<W>,
	) -> Result<()>
	where
		T: TryInto<QueryTarget, Error: Into<crate::Error>>,
		V: serde::Serialize,
		W: serde::Serialize,
	{
		let query_target: QueryTarget = query_target.try_into().map_err(|e| e.into())?;
		let query_value: JsonValue =
			serde_json::to_value(query_value).map_err(crate::Error::InvalidJsonInQueryKey)?;
		let query_response = match query_response {
			Ok(v) => serde_json::to_value(v).map_err(crate::Error::InvalidJsonInQueryKey),
			Err(e) => Err(e),
		};
		self.0.insert((query_target, query_value), query_response);
		Ok(())
	}
}
