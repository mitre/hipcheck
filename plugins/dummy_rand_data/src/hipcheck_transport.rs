use crate::hipcheck::{Query as PluginQuery, QueryState};
use anyhow::{anyhow, Result};
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use tokio::sync::mpsc;
use tonic::{codec::Streaming, Status};

#[derive(Debug)]
pub struct Query {
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
			request,
			publisher: value.publisher_name,
			plugin: value.plugin_name,
			query: value.query_name,
			key,
			output,
		})
	}
}

type SessionTracker = HashMap<i32, mpsc::Sender<Option<PluginQuery>>>;

pub struct QuerySession {
	id: usize,
	tx: mpsc::Sender<Result<PluginQuery, Status>>,
	rx: mpsc::Receiver<Option<PluginQuery>>,
	// So that we can remove ourselves when we get dropped
	drop_tx: mpsc::Sender<i32>,
}

impl QuerySession {
	pub fn id(&self) -> usize {
		self.id
	}

	// Roughly equivalent to TryFrom, but the `id` field value
	// comes from the QuerySession
	fn convert(&self, value: Query) -> Result<PluginQuery> {
		let state_enum = match value.request {
			true => QueryState::QuerySubmit,
			false => QueryState::QueryReplyComplete,
		};

		let key = serde_json::to_string(&value.key)?;
		let output = serde_json::to_string(&value.output)?;

		Ok(PluginQuery {
			id: self.id() as i32,
			state: state_enum as i32,
			publisher_name: value.publisher,
			plugin_name: value.plugin,
			query_name: value.query,
			key,
			output,
		})
	}

	async fn recv_raw(&mut self) -> Result<Option<VecDeque<PluginQuery>>> {
		let mut out = VecDeque::new();

		eprintln!("RAND-session: awaiting raw rx recv");

		let opt_first = self
			.rx
			.recv()
			.await
			.ok_or(anyhow!("session channel closed unexpectedly"))?;

		let Some(first) = opt_first else {
			// Underlying gRPC channel closed
			return Ok(None);
		};
		eprintln!("RAND-session: got first msg");
		out.push_back(first);

		// If more messages in the queue, opportunistically read more
		loop {
			eprintln!("RAND-session: trying to get additional msg");

			match self.rx.try_recv() {
				Ok(Some(msg)) => {
					out.push_back(msg);
				}
				Ok(None) => {
					eprintln!("warning: None received, gRPC channel closed. we may not close properly if None is not returned again");
					break;
				}
				// Whether empty or disconnected, we return what we have
				Err(_) => {
					break;
				}
			}
		}

		eprintln!("RAND-session: got {} msgs", out.len());
		Ok(Some(out))
	}

	pub async fn send(&self, query: Query) -> Result<()> {
		eprintln!("RAND-session: sending query");
		let query: PluginQuery = self.convert(query)?;
		self.tx.send(Ok(query)).await?;
		Ok(())
	}

	pub async fn recv(&mut self) -> Result<Option<Query>> {
		use QueryState::*;

		eprintln!("RAND-session: calling recv_raw");
		let Some(mut msg_chunks) = self.recv_raw().await? else {
			return Ok(None);
		};
		let mut raw = msg_chunks.pop_front().unwrap();
		eprintln!("RAND-session: recv got raw {raw:?}");

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
						match self.recv_raw().await? {
							Some(x) => {
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
					self.id
				));
			}
		}

		raw.try_into().map(Some)
	}
}

impl Drop for QuerySession {
	// Notify to have self removed from session tracker
	fn drop(&mut self) {
		use mpsc::error::TrySendError;
		let raw_id = self.id as i32;

		while let Err(e) = self.drop_tx.try_send(self.id as i32) {
			match e {
				TrySendError::Closed(_) => {
					break;
				}
				TrySendError::Full(_) => (),
			}
		}
	}
}

#[derive(Debug)]
pub struct HcSessionSocket {
	tx: mpsc::Sender<Result<PluginQuery, Status>>,
	rx: Streaming<PluginQuery>,
	drop_tx: mpsc::Sender<i32>,
	drop_rx: mpsc::Receiver<i32>,
	sessions: SessionTracker,
}

impl HcSessionSocket {
	pub fn new(tx: mpsc::Sender<Result<PluginQuery, Status>>, rx: Streaming<PluginQuery>) -> Self {
		// channel for QuerySession objects to notify us they dropped
		// @Todo - make this configurable
		let (drop_tx, drop_rx) = mpsc::channel(10);
		Self {
			tx,
			rx,
			drop_tx,
			drop_rx,
			sessions: HashMap::new(),
		}
	}

	fn cleanup_sessions(&mut self) {
		// Pull off all existing drop notifications
		while let Ok(id) = self.drop_rx.try_recv() {
			if self.sessions.remove(&id).is_none() {
				eprintln!(
					"WARNING: HcSessionSocket got request to drop a session that does not exist"
				);
			} else {
				eprintln!("Cleaned up session {id}");
			}
		}
	}

	pub async fn listen(&mut self) -> Result<Option<QuerySession>> {
		loop {
			eprintln!("RAND: listening");

			let Some(raw) = self.rx.message().await? else {
				return Ok(None);
			};

			// While we were waiting for a message, some session objects may have
			// dropped, handle them before we look at the ID of this message.
            // The downside of this strategy is that once we receive our last message,
            // we won't clean up any sessions that close after
			self.cleanup_sessions();

			let id = raw.id;

			// If there is already a session with this ID, forward msg
			if let Some(tx) = self.sessions.get_mut(&id) {
				eprintln!("RAND-listen: forwarding message to session {id}");

				if let Err(e) = tx.send(Some(raw)).await {
					eprintln!("Error forwarding msg to session {id}");
					self.sessions.remove(&id);
				};
			// If got a new query ID, create session
			} else if raw.state() == QueryState::QuerySubmit {
				eprintln!("RAND-listen: creating new session {id}");

				let (in_tx, rx) = mpsc::channel::<Option<PluginQuery>>(10);
				let tx = self.tx.clone();

				let session = QuerySession {
					id: id as usize,
					tx,
					rx,
					drop_tx: self.drop_tx.clone(),
				};

				in_tx
					.send(Some(raw))
					.await
					.expect("Failed sending message to newly created Session, should never happen");

				eprintln!("RAND-listen: adding new session {id} to tracker");
				self.sessions.insert(id, in_tx);

				return Ok(Some(session));
			} else {
				eprintln!("Got query with id {}, does not match existing session and is not new QuerySubmit", id);
			}
		}
	}
}
