use crate::{JsonValue, QueryTarget, Result};
use std::collections::HashMap;

pub struct MockResponses(pub(crate) HashMap<(QueryTarget, JsonValue), Result<JsonValue>>);

impl MockResponses {
	pub fn new() -> Self {
		Self(HashMap::new())
	}
}

#[cfg(feature = "mock_engine")]
impl MockResponses {
	fn inner_insert(
		mut self,
		query_target: QueryTarget,
		query_value: JsonValue,
		query_response: Result<JsonValue>,
	) -> Result<Self> {
		self.0.insert((query_target, query_value), query_response);
		Ok(self)
	}

	pub fn insert<T, V, W>(
		self,
		query_target: T,
		query_value: V,
		query_response: Result<W>,
	) -> Result<Self>
	where
		T: TryInto<QueryTarget, Error: Into<crate::Error>>,
		V: Into<JsonValue>,
		W: Into<JsonValue>,
	{
		let query_target: QueryTarget = query_target.try_into().map_err(|e| e.into())?;
		let query_value: JsonValue = query_value.into();
		let query_response = query_response.map(|v| v.into());
		self.inner_insert(query_target, query_value, query_response)
	}
}
