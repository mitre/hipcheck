use crate::hipcheck::{Query as PluginQuery, QueryState};
use anyhow::{anyhow, Result};
use indexmap::map::IndexMap;
use serde_json::Value;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
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
	type Error = anyhow::Error;
	fn try_from(value: PluginQuery) -> Result<Query> {
		use QueryState::*;
		let request = match TryInto::<QueryState>::try_into(value.state)? {
			QueryUnspecified => return Err(anyhow!("unspecified error from plugin")),
			QueryReplyInProgress => {
				return Err(anyhow!(
					"invalid state QueryReplyInProgress for conversion to Query"
				))
			}
			QueryReplyComplete => false,
			QuerySubmit => true,
		};
		let key: Value = serde_json::from_str(value.key.as_str())?;
		let output: Value = serde_json::from_str(value.output.as_str())?;
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
	type Error = anyhow::Error;
	fn try_from(value: Query) -> Result<PluginQuery> {
		let state_enum = match value.request {
			true => QueryState::QuerySubmit,
			false => QueryState::QueryReplyComplete,
		};
		let key = serde_json::to_string(&value.key)?;
		let output = serde_json::to_string(&value.output)?;
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

#[derive(Clone, Debug)]
pub struct HcTransport {
	tx: mpsc::Sender<Result<PluginQuery, Status>>,
	rx: Arc<Mutex<MultiplexedQueryReceiver>>,
}
impl HcTransport {
	pub fn new(rx: Streaming<PluginQuery>, tx: mpsc::Sender<Result<PluginQuery, Status>>) -> Self {
		HcTransport {
			rx: Arc::new(Mutex::new(MultiplexedQueryReceiver::new(rx))),
			tx,
		}
	}
	pub async fn send(&self, query: Query) -> Result<()> {
		let query: PluginQuery = query.try_into()?;
		self.tx.send(Ok(query)).await?;
		Ok(())
	}
	pub async fn recv_new(&self) -> Result<Option<Query>> {
		let mut rx_handle = self.rx.lock().await;
		match rx_handle.recv_new().await? {
			Some(msg) => msg.try_into().map(Some),
			None => Ok(None),
		}
	}
	pub async fn recv(&self, id: usize) -> Result<Option<Query>> {
		use QueryState::*;
		let id = id as i32;
		let mut rx_handle = self.rx.lock().await;
		let Some(mut msg_chunks) = rx_handle.recv(id).await? else {
			return Ok(None);
		};
		drop(rx_handle);
		let mut raw = msg_chunks.pop_front().unwrap();
		let mut state: QueryState = raw.state.try_into()?;

		// If response is the first of a set of chunks, handle
		if matches!(state, QueryReplyInProgress) {
			while matches!(state, QueryReplyInProgress) {
				// We expect another message. Pull it off the existing queue,
				// or get a new one if we have run out
				let next = match msg_chunks.pop_front() {
					Some(msg) => msg,
					None => {
						// We ran out of messages, get a new batch
						let mut rx_handle = self.rx.lock().await;
						match rx_handle.recv(id).await? {
							Some(x) => {
								drop(rx_handle);
								msg_chunks = x;
							}
							None => {
								return Ok(None);
							}
						};
						msg_chunks.pop_front().unwrap()
					}
				};
				// By now we have our "next" message
				state = next.state.try_into()?;
				match state {
					QueryUnspecified => return Err(anyhow!("unspecified error from plugin")),
					QuerySubmit => {
						return Err(anyhow!(
							"plugin sent QuerySubmit state when reply chunk expected"
						))
					}
					QueryReplyInProgress | QueryReplyComplete => {
						raw.output.push_str(next.output.as_str());
					}
				};
			}
			// Sanity check - after we've left this loop, there should be no left over message
			if !msg_chunks.is_empty() {
				return Err(anyhow!(
					"received additional messages for id '{}' after QueryComplete status message",
					id
				));
			}
		}
		raw.try_into().map(Some)
	}
}

#[derive(Debug)]
pub struct MultiplexedQueryReceiver {
	rx: Streaming<PluginQuery>,
	// Unlike in HipCheck, backlog is an IndexMap to ensure the earliest received
	// requests are handled first
	backlog: IndexMap<i32, VecDeque<PluginQuery>>,
}
impl MultiplexedQueryReceiver {
	pub fn new(rx: Streaming<PluginQuery>) -> Self {
		Self {
			rx,
			backlog: IndexMap::new(),
		}
	}
	pub async fn recv_new(&mut self) -> Result<Option<PluginQuery>> {
		let opt_unhandled = self.backlog.iter().find(|(k, v)| {
			if let Some(req) = v.front() {
				return req.state() == QueryState::QuerySubmit;
			}
			false
		});
		if let Some((k, v)) = opt_unhandled {
			let id: i32 = *k;
			let mut vec = self.backlog.shift_remove(&id).unwrap();
			// @Note - for now QuerySubmit doesn't chunk so we shouldn't expect
			// multiple messages in the backlog for a new request
			assert!(vec.len() == 1);
			return Ok(vec.pop_front());
		}
		// No backlog message, need to operate the receiver
		loop {
			let Some(raw) = self.rx.message().await? else {
				// gRPC channel was closed
				return Ok(None);
			};
			if raw.state() == QueryState::QuerySubmit {
				return Ok(Some(raw));
			}
			match self.backlog.get_mut(&raw.id) {
				Some(vec) => {
					vec.push_back(raw);
				}
				None => {
					self.backlog.insert(raw.id, VecDeque::from([raw]));
				}
			}
		}
	}
	// @Invariant - this function will never return an empty VecDeque
	pub async fn recv(&mut self, id: i32) -> Result<Option<VecDeque<PluginQuery>>> {
		// If we have 1+ messages on backlog for `id`, return them all,
		// no need to waste time with successive calls
		if let Some(msgs) = self.backlog.shift_remove(&id) {
			return Ok(Some(msgs));
		}
		// No backlog message, need to operate the receiver
		loop {
			let Some(raw) = self.rx.message().await? else {
				// gRPC channel was closed
				return Ok(None);
			};
			if raw.id == id {
				return Ok(Some(VecDeque::from([raw])));
			}
			match self.backlog.get_mut(&raw.id) {
				Some(vec) => {
					vec.push_back(raw);
				}
				None => {
					self.backlog.insert(raw.id, VecDeque::from([raw]));
				}
			}
		}
	}
}
