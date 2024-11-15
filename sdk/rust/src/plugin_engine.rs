// SPDX-License-Identifier: Apache-2.0

use crate::proto::QueryState;
use crate::{
	error::{Error, Result},
	proto::{
		self, InitiateQueryProtocolRequest, InitiateQueryProtocolResponse, Query as PluginQuery,
	},
	QueryTarget,
};
use crate::{mock::MockResponses, JsonValue, Plugin};
use anyhow::anyhow;
use futures::Stream;
use serde::Serialize;
use serde_json::{json, Value};
use std::sync::Arc;
use std::{
	collections::{HashMap, VecDeque},
	future::poll_fn,
	ops::Not,
	pin::Pin,
	result::Result as StdResult,
};
use tokio::sync::mpsc::{self, error::TrySendError};
use tonic::Status;

const GRPC_MAX_SIZE: usize = 1024 * 1024 * 4; // 4MB
const GRPC_EFFECTIVE_MAX_SIZE: usize = GRPC_MAX_SIZE - 1024; // Minus one KB

/// Try to drain `max` bytes from `buf`, or the full string, whichever is shortest.
/// If `max` bytes is somewhere within `buf` but lands within a char boundary,
/// walk backwards to the start of the previous char. Returns the substring
/// drained from `buf`.
fn drain_at_most_n_bytes(buf: &mut String, max: usize) -> Result<String> {
	let mut to_drain = std::cmp::min(buf.bytes().len(), max);
	while to_drain > 0 && buf.is_char_boundary(to_drain).not() {
		to_drain -= 1;
	}
	if to_drain == 0 {
		return Err(anyhow!("Could not drain any whole char from string").into());
	}
	Ok(buf.drain(0..to_drain).collect::<String>())
}

fn estimate_size(msg: &PluginQuery) -> usize {
	msg.key.bytes().len()
		+ msg.output.bytes().len()
		+ msg.concern.iter().map(|x| x.bytes().len()).sum::<usize>()
}

fn chunk_with_size(msg: PluginQuery, max_est_size: usize) -> Result<Vec<PluginQuery>> {
	// Chunking only does something on response objects, mostly because
	// we don't have a state to represent "SubmitInProgress"
	if msg.state == QueryState::Submit as i32 {
		return Ok(vec![msg]);
	}

	let mut out: Vec<PluginQuery> = vec![];
	let mut base: PluginQuery = msg;

	// Track whether we did anything on each iteration to avoid infinite loop
	let mut made_progress = true;
	while estimate_size(&base) > max_est_size {
		log::trace!("Estimated size is too large, chunking");
		if !made_progress {
			log::error!("Message could not be chunked");
			return Err(Error::UnspecifiedQueryState);
		}

		made_progress = false;
		// For this loop, we want to take at most MAX_SIZE bytes because that's
		// all that can fit in a PluginQuery
		let mut remaining = max_est_size;
		let mut query = proto::Query {
			id: base.id,
			state: QueryState::ReplyInProgress as i32,
			publisher_name: base.publisher_name.clone(),
			plugin_name: base.plugin_name.clone(),
			query_name: base.query_name.clone(),
			key: String::new(),
			output: String::new(),
			concern: vec![],
		};

		if remaining > 0 && base.key.bytes().len() > 0 {
			// steal from key
			query.key = drain_at_most_n_bytes(&mut base.key, remaining)?;
			remaining -= query.key.bytes().len();
			made_progress = true;
		}

		if remaining > 0 && base.output.bytes().len() > 0 {
			// steal from output
			query.output = drain_at_most_n_bytes(&mut base.output, remaining)?;
			remaining -= query.output.bytes().len();
			made_progress = true;
		}

		let mut l = base.concern.len();
		// While we still want to steal more bytes and we have more elements of
		// `concern` to possibly steal
		while remaining > 0 && l > 0 {
			let i = l - 1;

			let c_bytes = base.concern.get(i).unwrap().bytes().len();

			if c_bytes > max_est_size {
				return Err(anyhow!("Query cannot be chunked, there is a concern that is larger than max chunk size").into());
			} else if c_bytes <= remaining {
				// steal this concern
				let concern = base.concern.swap_remove(i);
				query.concern.push(concern);
				remaining -= c_bytes;
				made_progress = true;
			}
			// since we use `swap_remove`, whether or not we stole a concern we know the element
			// currently at `i` is too big for `remainder` (since if we removed, the element at `i`
			// now is one we already passed on)
			l -= 1;
		}

		out.push(query);
	}
	out.push(base);
	Ok(out)
}

fn chunk(msg: PluginQuery) -> Result<Vec<PluginQuery>> {
	chunk_with_size(msg, GRPC_EFFECTIVE_MAX_SIZE)
}

impl From<Status> for Error {
	fn from(_value: Status) -> Error {
		// TODO: higher-fidelity handling?
		Error::SessionChannelClosed
	}
}

#[derive(Debug)]
struct Query {
	direction: QueryDirection,
	publisher: String,
	plugin: String,
	query: String,
	key: Value,
	output: Value,
	concerns: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
enum QueryDirection {
	Request,
	Response,
}

impl TryFrom<QueryState> for QueryDirection {
	type Error = Error;

	fn try_from(value: QueryState) -> std::result::Result<Self, Self::Error> {
		match value {
			QueryState::Unspecified => Err(Error::UnspecifiedQueryState),
			QueryState::Submit => Ok(QueryDirection::Request),
			QueryState::ReplyInProgress => Err(Error::UnexpectedReplyInProgress),
			QueryState::ReplyComplete => Ok(QueryDirection::Response),
		}
	}
}

impl From<QueryDirection> for QueryState {
	fn from(value: QueryDirection) -> Self {
		match value {
			QueryDirection::Request => QueryState::Submit,
			QueryDirection::Response => QueryState::ReplyComplete,
		}
	}
}

impl TryFrom<PluginQuery> for Query {
	type Error = Error;

	fn try_from(value: PluginQuery) -> Result<Query> {
		Ok(Query {
			direction: QueryDirection::try_from(value.state())?,
			publisher: value.publisher_name,
			plugin: value.plugin_name,
			query: value.query_name,
			key: serde_json::from_str(value.key.as_str()).map_err(Error::InvalidJsonInQueryKey)?,
			output: serde_json::from_str(value.output.as_str())
				.map_err(Error::InvalidJsonInQueryOutput)?,
			concerns: value.concern,
		})
	}
}

type SessionTracker = HashMap<i32, mpsc::Sender<Option<PluginQuery>>>;

/// The handle that a `Query::run()` function can use to request information from other Hipcheck
/// plugins in order to fulfill a query.
pub struct PluginEngine {
	id: usize,
	tx: mpsc::Sender<StdResult<InitiateQueryProtocolResponse, Status>>,
	rx: mpsc::Receiver<Option<PluginQuery>>,
	concerns: Vec<String>,
	// So that we can remove ourselves when we get dropped
	drop_tx: mpsc::Sender<i32>,
	/// when unit testing, this enables the user to mock plugin responses to various inputs
	mock_responses: MockResponses,
}

impl PluginEngine {
	#[cfg(feature = "mock_engine")]
	pub fn mock(mock_responses: MockResponses) -> Self {
		mock_responses.into()
	}

	/// Query another Hipcheck plugin `target` with key `input`. On success, the JSONified result
	/// of the query is returned. `target` will often be a string of the format
	/// `"publisher/plugin[/query]"`, where the bracketed substring is optional if the plugin's
	/// default query endpoint is desired. `input` must of a type implementing `Into<JsonValue>`,
	/// which can be done by deriving or implementing `serde::Serialize`.
	pub async fn query<T, V>(&mut self, target: T, input: V) -> Result<JsonValue>
	where
		T: TryInto<QueryTarget, Error: Into<Error>>,
		V: Serialize,
	{
		let query_target: QueryTarget = target.try_into().map_err(|e| e.into())?;
		let input: JsonValue = serde_json::to_value(input).map_err(Error::InvalidJsonInQueryKey)?;

		async fn query_inner(
			engine: &mut PluginEngine,
			target: QueryTarget,
			input: JsonValue,
		) -> Result<JsonValue> {
			// If doing a mock engine, look to the `mock_responses` field for the query answer
			if cfg!(feature = "mock_engine") {
				match engine.mock_responses.0.get(&(target, input)) {
					Some(res) => {
						match res {
							Ok(val) => Ok(val.clone()),
							// TODO: since Error is not Clone, is there a better way to deal with this
							Err(_) => Err(Error::UnexpectedPluginQueryInputFormat),
						}
					}
					None => Err(Error::UnknownPluginQuery),
				}
			}
			// Normal execution, send messages to hipcheck core to query other plugin
			else {
				let query = Query {
					direction: QueryDirection::Request,
					publisher: target.publisher,
					plugin: target.plugin,
					query: target.query.unwrap_or_else(|| "".to_owned()),
					key: input,
					output: json!(Value::Null),
					concerns: vec![],
				};
				engine.send(query).await?;
				let response = engine.recv().await?;
				match response {
					Some(response) => Ok(response.output),
					None => Err(Error::SessionChannelClosed),
				}
			}
		}
		query_inner(self, query_target, input).await
	}

	fn id(&self) -> usize {
		self.id
	}

	// Roughly equivalent to TryFrom, but the `id` field value
	// comes from the QuerySession
	fn convert(&self, value: Query) -> Result<PluginQuery> {
		let state: QueryState = value.direction.into();
		let key = serde_json::to_string(&value.key).map_err(Error::InvalidJsonInQueryKey)?;
		let output =
			serde_json::to_string(&value.output).map_err(Error::InvalidJsonInQueryOutput)?;

		Ok(PluginQuery {
			id: self.id() as i32,
			state: state as i32,
			publisher_name: value.publisher,
			plugin_name: value.plugin,
			query_name: value.query,
			key,
			output,
			concern: value.concerns,
		})
	}

	async fn recv_raw(&mut self) -> Result<Option<VecDeque<PluginQuery>>> {
		let mut out = VecDeque::new();

		log::trace!("SDK: awaiting raw rx recv");

		let opt_first = self.rx.recv().await.ok_or(Error::SessionChannelClosed)?;

		let Some(first) = opt_first else {
			// Underlying gRPC channel closed
			return Ok(None);
		};
		out.push_back(first);

		// If more messages in the queue, opportunistically read more
		loop {
			match self.rx.try_recv() {
				Ok(Some(msg)) => {
					out.push_back(msg);
				}
				Ok(None) => {
					log::warn!("None received, gRPC channel closed. we may not close properly if None is not returned again");
					break;
				}
				// Whether empty or disconnected, we return what we have
				Err(_) => {
					break;
				}
			}
		}

		Ok(Some(out))
	}

	// Send a gRPC query from plugin to the hipcheck server
	async fn send(&self, query: Query) -> Result<()> {
		let queries = chunk(self.convert(query)?)?;
		for pq in queries {
			let query = InitiateQueryProtocolResponse { query: Some(pq) };
			self.tx
				.send(Ok(query))
				.await
				.map_err(Error::FailedToSendQueryFromSessionToServer)?;
		}
		Ok(())
	}

	async fn send_session_err<P>(&mut self) -> crate::error::Result<()>
	where
		P: Plugin,
	{
		let query = proto::Query {
			id: self.id() as i32,
			state: QueryState::Unspecified as i32,
			publisher_name: P::PUBLISHER.to_owned(),
			plugin_name: P::NAME.to_owned(),
			query_name: "".to_owned(),
			key: json!(Value::Null).to_string(),
			output: json!(Value::Null).to_string(),
			concern: self.take_concerns(),
		};
		self.tx
			.send(Ok(InitiateQueryProtocolResponse { query: Some(query) }))
			.await
			.map_err(Error::FailedToSendQueryFromSessionToServer)
	}

	async fn recv(&mut self) -> Result<Option<Query>> {
		let Some(mut msg_chunks) = self.recv_raw().await? else {
			return Ok(None);
		};

		let mut raw: PluginQuery = msg_chunks.pop_front().unwrap();

		let mut state: QueryState = raw
			.state
			.try_into()
			.map_err(|_| Error::UnspecifiedQueryState)?;

		// If response is the first of a set of chunks, handle
		if matches!(state, QueryState::ReplyInProgress) {
			while matches!(state, QueryState::ReplyInProgress) {
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
				state = next
					.state
					.try_into()
					.map_err(|_| Error::UnspecifiedQueryState)?;
				match state {
					QueryState::Unspecified => return Err(Error::UnspecifiedQueryState),
					QueryState::Submit => return Err(Error::ReceivedSubmitWhenExpectingReplyChunk),
					QueryState::ReplyInProgress | QueryState::ReplyComplete => {
						if state == QueryState::ReplyComplete {
							raw.state = QueryState::ReplyComplete.into();
						}
						raw.key.push_str(next.key.as_str());
						raw.output.push_str(next.output.as_str());
						raw.concern.extend_from_slice(next.concern.as_slice());
					}
				};
			}

			// Sanity check - after we've left this loop, there should be no left over message
			if msg_chunks.is_empty().not() {
				return Err(Error::MoreAfterQueryComplete { id: self.id });
			}
		}

		raw.try_into().map(Some)
	}

	async fn handle_session_fallible<P>(&mut self, plugin: Arc<P>) -> crate::error::Result<()>
	where
		P: Plugin,
	{
		let Some(query) = self.recv().await? else {
			return Err(Error::SessionChannelClosed);
		};

		if query.direction == QueryDirection::Response {
			return Err(Error::ReceivedSubmitWhenExpectingReplyChunk);
		}

		let name = query.query;
		let key = query.key;

		// if we find the plugin by name, run it
		// if not, check if there is a default plugin and run that one
		// otherwise error out
		let query = plugin
			.queries()
			.filter_map(|x| if x.name == name { Some(x.inner) } else { None })
			.next()
			.or_else(|| plugin.default_query())
			.ok_or_else(|| Error::UnknownPluginQuery)?;

		let value = query.run(self, key).await?;

		let query = Query {
			direction: QueryDirection::Response,
			publisher: P::PUBLISHER.to_owned(),
			plugin: P::NAME.to_owned(),
			query: name.to_owned(),
			key: json!(Value::Null),
			output: value,
			concerns: self.take_concerns(),
		};

		self.send(query).await
	}

	async fn handle_session<P>(&mut self, plugin: Arc<P>)
	where
		P: Plugin,
	{
		use crate::error::Error::*;
		if let Err(e) = self.handle_session_fallible(plugin).await {
			let res_err_send = match e {
				FailedToSendQueryFromSessionToServer(_) => {
					log::error!("Failed to send message to Hipcheck core, analysis will hang.");
					return;
				}
				other => {
					log::error!("{}", other);
					self.send_session_err::<P>().await
				}
			};
			if res_err_send.is_err() {
				log::error!("Failed to send message to Hipcheck core, analysis will hang.");
			}
		}
	}

	pub fn record_concern<S: AsRef<str>>(&mut self, concern: S) {
		fn inner(engine: &mut PluginEngine, concern: &str) {
			engine.concerns.push(concern.to_owned());
		}
		inner(self, concern.as_ref())
	}

	pub fn take_concerns(&mut self) -> Vec<String> {
		self.concerns.drain(..).collect()
	}
}

#[cfg(feature = "mock_engine")]
impl From<MockResponses> for PluginEngine {
	fn from(value: MockResponses) -> Self {
		let (tx, _) = mpsc::channel(1);
		let (_, rx) = mpsc::channel(1);
		let (drop_tx, _) = mpsc::channel(1);

		Self {
			id: 0,
			concerns: vec![],
			tx,
			rx,
			drop_tx,
			mock_responses: value,
		}
	}
}

impl Drop for PluginEngine {
	// Notify to have self removed from session tracker
	fn drop(&mut self) {
		if cfg!(feature = "mock_engine") {
			// "use" drop_tx to prevent 'unused' warning. Less messy than trying to gate the
			// existence of "drop_tx" var itself.
			let _ = self.drop_tx.max_capacity();
		} else {
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
}

type PluginQueryStream = Box<
	dyn Stream<Item = StdResult<InitiateQueryProtocolRequest, Status>> + Send + Unpin + 'static,
>;

pub(crate) struct HcSessionSocket {
	tx: mpsc::Sender<StdResult<InitiateQueryProtocolResponse, Status>>,
	rx: PluginQueryStream,
	drop_tx: mpsc::Sender<i32>,
	drop_rx: mpsc::Receiver<i32>,
	sessions: SessionTracker,
}

// This is implemented manually since the stream trait object
// can't impl `Debug`.
impl std::fmt::Debug for HcSessionSocket {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("HcSessionSocket")
			.field("tx", &self.tx)
			.field("rx", &"<rx>")
			.field("drop_tx", &self.drop_tx)
			.field("drop_rx", &self.drop_rx)
			.field("sessions", &self.sessions)
			.finish()
	}
}

impl HcSessionSocket {
	pub(crate) fn new(
		tx: mpsc::Sender<StdResult<InitiateQueryProtocolResponse, Status>>,
		rx: impl Stream<Item = StdResult<InitiateQueryProtocolRequest, Status>> + Send + Unpin + 'static,
	) -> Self {
		// channel for QuerySession objects to notify us they dropped
		// TODO: make this configurable
		let (drop_tx, drop_rx) = mpsc::channel(10);
		Self {
			tx,
			rx: Box::new(rx),
			drop_tx,
			drop_rx,
			sessions: HashMap::new(),
		}
	}

	/// Clean up completed sessions by going through all drop messages.
	fn cleanup_sessions(&mut self) {
		while let Ok(id) = self.drop_rx.try_recv() {
			match self.sessions.remove(&id) {
				Some(_) => log::trace!("Cleaned up session {id}"),
				None => {
					log::warn!("HcSessionSocket got request to drop a session that does not exist")
				}
			}
		}
	}

	async fn message(&mut self) -> StdResult<Option<PluginQuery>, Status> {
		let fut = poll_fn(|cx| Pin::new(&mut *self.rx).poll_next(cx));

		match fut.await {
			Some(Ok(m)) => Ok(m.query),
			Some(Err(e)) => Err(e),
			None => Ok(None),
		}
	}

	pub(crate) async fn listen(&mut self) -> Result<Option<PluginEngine>> {
		loop {
			let Some(raw) = self.message().await.map_err(Error::from)? else {
				return Ok(None);
			};
			let id = raw.id;

			// While we were waiting for a message, some session objects may have
			// dropped, handle them before we look at the ID of this message.
			// The downside of this strategy is that once we receive our last message,
			// we won't clean up any sessions that close after
			self.cleanup_sessions();

			match self.decide_action(&raw) {
				Ok(HandleAction::ForwardMsgToExistingSession(tx)) => {
					log::trace!("SDK: forwarding message to session {id}");

					if let Err(_e) = tx.send(Some(raw)).await {
						log::error!("Error forwarding msg to session {id}");
						self.sessions.remove(&id);
					};
				}
				Ok(HandleAction::CreateSession) => {
					log::trace!("SDK: creating new session {id}");

					let (in_tx, rx) = mpsc::channel::<Option<PluginQuery>>(10);
					let tx = self.tx.clone();

					let session = PluginEngine {
						id: id as usize,
						concerns: vec![],
						tx,
						rx,
						drop_tx: self.drop_tx.clone(),
						mock_responses: MockResponses::new(),
					};

					in_tx.send(Some(raw)).await.expect(
						"Failed sending message to newly created Session, should never happen",
					);

					log::trace!("SDK: adding new session {id} to tracker");
					self.sessions.insert(id, in_tx);

					return Ok(Some(session));
				}
				Err(e) => log::error!("{}", e),
			}
		}
	}

	fn decide_action(&mut self, query: &PluginQuery) -> Result<HandleAction<'_>> {
		if let Some(tx) = self.sessions.get_mut(&query.id) {
			return Ok(HandleAction::ForwardMsgToExistingSession(tx));
		}

		if query.state() == QueryState::Submit {
			return Ok(HandleAction::CreateSession);
		}

		Err(Error::ReceivedReplyWhenExpectingRequest)
	}

	pub(crate) async fn run<P>(&mut self, plugin: Arc<P>) -> Result<()>
	where
		P: Plugin,
	{
		loop {
			let Some(mut engine) = self
				.listen()
				.await
				.map_err(|_| Error::SessionChannelClosed)?
			else {
				log::trace!("Channel closed by remote");
				break;
			};

			let cloned_plugin = plugin.clone();
			tokio::spawn(async move {
				engine.handle_session(cloned_plugin).await;
			});
		}

		Ok(())
	}
}

enum HandleAction<'s> {
	ForwardMsgToExistingSession(&'s mut mpsc::Sender<Option<PluginQuery>>),
	CreateSession,
}

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn test_bounded_char_draining() {
		let orig_key = "aこれは実験です".to_owned();

		let mut key = orig_key.clone();
		let res = drain_at_most_n_bytes(&mut key, 10).unwrap();
		let num_bytes = res.bytes().len();

		assert!(num_bytes > 0 && num_bytes <= 10);

		// Make sure the drained str + retained str combine to re-create original
		let mut reassembled = res.clone();
		reassembled.push_str(&key);

		assert_eq!(orig_key, reassembled);
	}

	#[test]
	fn test_chunking() {
		let query = PluginQuery {
			id: 0,
			state: QueryState::ReplyComplete as i32,
			publisher_name: "".to_owned(),
			plugin_name: "".to_owned(),
			query_name: "".to_owned(),
			// This key will cause the chunk not to occur on a char boundary
			key: "aこれは実験です".to_owned(),
			output: "".to_owned(),
			concern: vec!["< 10".to_owned(), "0123456789".to_owned()],
		};
		let res = match chunk_with_size(query, 10) {
			Ok(r) => r,
			Err(e) => {
				println!("{e}");
				assert!(false);
				return;
			}
		};
		assert_eq!(res.len(), 4);
	}
}
