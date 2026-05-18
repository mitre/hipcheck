// SPDX-License-Identifier: Apache-2.0

pub(crate) mod mock_responses;
pub(crate) mod plugin_engine;
pub(crate) mod query_builder;
pub(crate) mod session_socket;

pub use crate::engine::plugin_engine::PluginEngine;

#[cfg(test)]
mod test {
	use super::*;
	use serde_json::Value as JsonValue;

	#[cfg(feature = "mock_engine")]
	#[tokio::test]
	async fn test_query_builder() {
		let mut mock_responses = mock_responses::MockResponses::new();
		mock_responses
			.insert("mitre/foo", "abcd", Ok(1234))
			.unwrap();
		mock_responses
			.insert("mitre/foo", "efgh", Ok(5678))
			.unwrap();
		let mut engine = PluginEngine::mock(mock_responses);
		let mut builder = engine.batch("mitre/foo").unwrap();
		let idx = builder.query("abcd".into());
		assert_eq!(idx, 0);
		let idx = builder.query("efgh".into());
		assert_eq!(idx, 1);
		let response = builder.send().await.unwrap();
		assert_eq!(
			response.first().unwrap(),
			&<i32 as Into<JsonValue>>::into(1234)
		);
		assert_eq!(
			response.get(1).unwrap(),
			&<i32 as Into<JsonValue>>::into(5678)
		);
	}
}
