use crate::hipcheck::{Query as PluginQuery, QueryState};
use serde_json::Value;
use tokio::sync::mpsc;
use tonic::{codec::Streaming, Status};

#[derive(Debug)]
pub struct Query {
	pub id: usize,
	// if false, response
	pub request: bool,
	pub publisher: String,
	pub plugin: String,
	pub query: String,
	pub key: Value,
	pub output: Value,
}
impl TryFrom<PluginQuery> for Query {
	type Error = String;
	fn try_from(value: PluginQuery) -> Result<Query, String> {
		use QueryState::*;
		let request =
			match TryInto::<QueryState>::try_into(value.state).map_err(|e| e.to_string())? {
				QueryUnspecified => return Err("unspecified error from plugin".into()),
				QueryReplyInProgress => {
					return Err("invalid state QueryReplyInProgress for conversion to Query".into())
				}
				QueryReplyComplete => false,
				QuerySubmit => true,
			};
		let key: Value = serde_json::from_str(value.key.as_str()).map_err(|e| e.to_string())?;
		let output: Value =
			serde_json::from_str(value.output.as_str()).map_err(|e| e.to_string())?;
		Ok(Query {
			id: value.id as usize,
			request,
			publisher: value.publisher_name,
			plugin: value.plugin_name,
			query: value.query_name,
			key,
			output,
		})
	}
}
impl TryFrom<Query> for PluginQuery {
	type Error = String;
	fn try_from(value: Query) -> Result<PluginQuery, String> {
		let state_enum = match value.request {
			true => QueryState::QuerySubmit,
			false => QueryState::QueryReplyComplete,
		};
		let key = serde_json::to_string(&value.key).map_err(|e| e.to_string())?;
		let output = serde_json::to_string(&value.output).map_err(|e| e.to_string())?;
		Ok(PluginQuery {
			id: value.id as i32,
			state: state_enum as i32,
			publisher_name: value.publisher,
			plugin_name: value.plugin,
			query_name: value.query,
			key,
			output,
		})
	}
}

pub struct HcTransport {
	rx: Streaming<PluginQuery>,
	tx: mpsc::Sender<Result<PluginQuery, Status>>,
}
impl HcTransport {
	pub fn new(rx: Streaming<PluginQuery>, tx: mpsc::Sender<Result<PluginQuery, Status>>) -> Self {
		HcTransport { rx, tx }
	}
	pub async fn send(&mut self, query: Query) -> Result<(), String> {
		let query: PluginQuery = query.try_into()?;
		self.tx
			.send(Ok(query))
			.await
			.map_err(|e| format!("sending query failed: {}", e))
	}
	pub async fn recv(&mut self) -> Result<Option<Query>, String> {
		use QueryState::*;
		let Some(mut raw) = self.rx.message().await.map_err(|e| e.to_string())? else {
			// gRPC channel was closed
			return Ok(None);
		};
		let mut state: QueryState =
			TryInto::<QueryState>::try_into(raw.state).map_err(|e| e.to_string())?;
		// As long as we expect successive chunks, keep receiving
		if matches!(state, QueryReplyInProgress) {
			while matches!(state, QueryReplyInProgress) {
				println!("Retrieving next response");
				let Some(next) = self.rx.message().await.map_err(|e| e.to_string())? else {
					return Err("plugin gRPC channel closed while sending chunked message".into());
				};
				// Assert that the ids are consistent
				if next.id != raw.id {
					return Err("msg ids from plugin do not match".into());
				}
				state = TryInto::<QueryState>::try_into(next.state).map_err(|e| e.to_string())?;
				match state {
					QueryUnspecified => return Err("unspecified error from plugin".to_owned()),
					QuerySubmit => {
						return Err(
							"plugin sent QuerySubmit state when reply chunk expected".to_owned()
						)
					}
					QueryReplyInProgress | QueryReplyComplete => {
						raw.output.push_str(next.output.as_str());
					}
				};
			}
		}
		raw.try_into().map(Some)
	}
}
