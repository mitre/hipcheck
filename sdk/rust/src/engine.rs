// SPDX-License-Identifier: Apache-2.0

use crate::{
	error::{Error, Result},
	JsonValue, Plugin, QueryTarget,
};
use futures::Stream;
use hipcheck_common::proto::{
	self, InitiateQueryProtocolRequest, InitiateQueryProtocolResponse, Query as PluginQuery,
	QueryState,
};
use hipcheck_common::{
	chunk::QuerySynthesizer,
	types::{Query, QueryDirection},
};
use serde::Serialize;
use std::sync::Arc;
use std::{
	collections::{HashMap, VecDeque},
	future::poll_fn,
	pin::Pin,
	result::Result as StdResult,
};
use tokio::sync::mpsc::{self, error::TrySendError};
use tonic::Status;

impl From<Status> for Error {
	fn from(_value: Status) -> Error {
		// TODO: higher-fidelity handling?
		Error::SessionChannelClosed
	}
}

type SessionTracker = HashMap<i32, mpsc::Sender<Option<PluginQuery>>>;

/// Manages a particular query session.
///
/// This struct invokes a `Query` trait object, passing a handle to itself to `Query::run()`. This
/// allows the query logic to request information from other Hipcheck plugins in order to complete.
pub struct PluginEngine {
	id: usize,
	tx: mpsc::Sender<StdResult<InitiateQueryProtocolResponse, Status>>,
	rx: mpsc::Receiver<Option<PluginQuery>>,
	concerns: Vec<String>,
	// So that we can remove ourselves when we get dropped
	drop_tx: mpsc::Sender<i32>,
	// When unit testing, this enables the user to mock plugin responses to various inputs
	mock_responses: MockResponses,
}

impl PluginEngine {
	#[cfg(feature = "mock_engine")]
	#[cfg_attr(docsrs, doc(cfg(feature = "mock_engine")))]
	/// Constructor for use in unit tests, `query()` function will reference this map instead of
	/// trying to connect to Hipcheck core for a response value
	pub fn mock(mock_responses: MockResponses) -> Self {
		mock_responses.into()
	}

	async fn query_inner(
		&mut self,
		target: QueryTarget,
		input: Vec<JsonValue>,
	) -> Result<Vec<JsonValue>> {
		// If doing a mock engine, look to the `mock_responses` field for the query answer
		if cfg!(feature = "mock_engine") {
			let mut results = Vec::with_capacity(input.len());
			for i in input {
				match self.mock_responses.0.get(&(target.clone(), i)) {
					Some(res) => {
						match res {
							Ok(val) => results.push(val.clone()),
							// TODO: since Error is not Clone, is there a better way to deal with this
							Err(_) => return Err(Error::UnexpectedPluginQueryInputFormat),
						}
					}
					None => return Err(Error::UnknownPluginQuery),
				}
			}
			Ok(results)
		}
		// Normal execution, send messages to hipcheck core to query other plugin
		else {
			let query = Query {
				id: 0,
				direction: QueryDirection::Request,
				publisher: target.publisher,
				plugin: target.plugin,
				query: target.query.unwrap_or_else(|| "".to_owned()),
				key: input,
				output: vec![],
				concerns: vec![],
			};
			self.send(query).await?;
			let response = self.recv().await?;
			match response {
				Some(response) => Ok(response.output),
				None => Err(Error::SessionChannelClosed),
			}
		}
	}

	/// Query another Hipcheck plugin `target` with key `input`. On success, the JSONified result
	/// of the query is returned. `target` will often be a string of the format
	/// `"publisher/plugin[/query]"`, where the bracketed substring is optional if the plugin's
	/// default query endpoint is desired. `input` must be of a type implementing `serde::Serialize`,
	pub async fn query<T, V>(&mut self, target: T, input: V) -> Result<JsonValue>
	where
		T: TryInto<QueryTarget, Error: Into<Error>>,
		V: Serialize,
	{
		let query_target: QueryTarget = target.try_into().map_err(|e| e.into())?;
		let input: JsonValue = serde_json::to_value(input).map_err(Error::InvalidJsonInQueryKey)?;
		// since there input had one value, there will only be one response
		let mut response = self.query_inner(query_target, vec![input]).await?;
		Ok(response.pop().unwrap())
	}

	/// Query another Hipcheck plugin `target` with Vec of `inputs`. On success, the JSONified result
	/// of the query is returned. `target` will often be a string of the format
	/// `"publisher/plugin[/query]"`, where the bracketed substring is optional if the plugin's
	/// default query endpoint is desired. `input` must be a Vec containing a type which implements `serde::Serialize`,
	pub async fn batch_query<T, V>(&mut self, target: T, keys: Vec<V>) -> Result<Vec<JsonValue>>
	where
		T: TryInto<QueryTarget, Error: Into<Error>>,
		V: Serialize,
	{
		let target: QueryTarget = target.try_into().map_err(|e| e.into())?;
		let mut input = Vec::with_capacity(keys.len());
		for key in keys {
			let jsonified_key = serde_json::to_value(key).map_err(Error::InvalidJsonInQueryKey)?;
			input.push(jsonified_key);
		}
		self.query_inner(target, input).await
	}

	fn id(&self) -> usize {
		self.id
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
	async fn send(&self, mut query: Query) -> Result<()> {
		query.id = self.id(); // incoming id value is just a placeholder
		let queries = hipcheck_common::chunk::prepare(query)?;
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
			key: vec![],
			output: vec![],
			concern: self.take_concerns(),
			split: false,
		};
		self.tx
			.send(Ok(InitiateQueryProtocolResponse { query: Some(query) }))
			.await
			.map_err(Error::FailedToSendQueryFromSessionToServer)
	}

	async fn recv(&mut self) -> Result<Option<Query>> {
		let mut synth = QuerySynthesizer::default();
		let mut res: Option<Query> = None;
		while res.is_none() {
			let Some(msg_chunks) = self.recv_raw().await? else {
				return Ok(None);
			};
			res = synth.add(msg_chunks.into_iter())?;
		}
		Ok(res)
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

		// Per RFD 0009, there should only be one query key per query
		if query.key.len() != 1 {
			return Err(Error::UnspecifiedQueryState);
		}
		let key = query.key.first().unwrap().clone();

		// if we find the plugin by name, run it
		// if not, check if there is a default plugin and run that one
		// otherwise error out
		let query = plugin
			.queries()
			.filter_map(|x| if x.name == name { Some(x.inner) } else { None })
			.next()
			.or_else(|| plugin.default_query())
			.ok_or_else(|| Error::UnknownPluginQuery)?;

		#[cfg(feature = "print-timings")]
		let _0 = crate::benchmarking::print_scope_time!(format!("{}/{}", P::NAME, name));

		let value = query.run(self, key).await?;

		#[cfg(feature = "print-timings")]
		drop(_0);

		let query = Query {
			id: self.id(),
			direction: QueryDirection::Response,
			publisher: P::PUBLISHER.to_owned(),
			plugin: P::NAME.to_owned(),
			query: name.to_owned(),
			key: vec![],
			output: vec![value],
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

	/// Records a string-like concern that will be emitted in the final Hipcheck report. Intended
	/// for use within a `Query` trait impl.
	pub fn record_concern<S: AsRef<str>>(&mut self, concern: S) {
		fn inner(engine: &mut PluginEngine, concern: &str) {
			engine.concerns.push(concern.to_owned());
		}
		inner(self, concern.as_ref())
	}

	#[cfg(feature = "mock_engine")]
	#[cfg_attr(docsrs, doc(cfg(feature = "mock_engine")))]
	/// Exposes the current set of concerns recorded by `PluginEngine`
	pub fn get_concerns(&self) -> &[String] {
		&self.concerns
	}

	fn take_concerns(&mut self) -> Vec<String> {
		self.concerns.drain(..).collect()
	}
}

#[cfg(feature = "mock_engine")]
#[cfg_attr(docsrs, doc(cfg(feature = "mock_engine")))]
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

		if [QueryState::SubmitInProgress, QueryState::SubmitComplete].contains(&query.state()) {
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
