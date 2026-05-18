// SPDX-License-Identifier: Apache-2.0

use crate::{
	engine::{mock_responses::MockResponses, query_builder::QueryBuilder},
	error::{Error, Result},
	plugin::Plugin,
	target::QueryTarget,
};
use hipcheck_common::proto::{
	self, InitiateQueryProtocolResponse, Query as PluginQuery, QueryState,
};
use hipcheck_common::{
	chunk::QuerySynthesizer,
	types::{Query, QueryDirection},
};
use serde::Serialize;
use serde_json::Value as JsonValue;
use std::{collections::VecDeque, result::Result as StdResult, sync::Arc};
use tokio::sync::mpsc::{self, error::TrySendError};
use tonic::Status;

/// Manages a particular query session.
///
/// This struct invokes a `Query` trait object, passing a handle to itself to `Query::run()`. This
/// allows the query logic to request information from other Hipcheck plugins in order to complete.
pub struct PluginEngine {
	/// The current ID, which increments through a series of calls/responses.
	pub(crate) id: i32,
	/// Transmitter, used to initiate queries.
	pub(crate) tx: mpsc::Sender<StdResult<InitiateQueryProtocolResponse, Status>>,
	/// Receiver, used to get back query responses.
	pub(crate) rx: mpsc::Receiver<Option<PluginQuery>>,
	/// Any "concerns" arising during execution. These are messages to be reported to the end-user
	/// to inform them how to take action based on a top-level query's results.
	pub(crate) concerns: Vec<String>,
	/// Transmitter used to indicate that *the current task* can be removed from the system when
	/// dropped.
	pub(crate) drop_tx: mpsc::Sender<i32>,
	/// When unit testing, this enables the user to mock plugin responses to various inputs
	pub(crate) mock_responses: MockResponses,
}

impl PluginEngine {
	// Note that the only public constructor we offer is `mock`, which is for use in tests (where
	// we want the user to be able to provide their own mock responses). This is because in normal
	// usage, the user is given an already-initialized `PluginEngine` as a parameter to any
	// query function; they don't make it themselves.

	#[cfg(feature = "mock_engine")]
	#[cfg_attr(docsrs, doc(cfg(feature = "mock_engine")))]
	/// Constructor for use in unit tests, `query()` function will reference this map instead of
	/// trying to connect to Hipcheck core for a response value
	pub fn mock(mock_responses: MockResponses) -> Self {
		mock_responses.into()
	}

	/// Start a "batch" query, which sends many queries as a single gRPC request.
	///
	/// Convenience function to expose a `QueryBuilder` to make it convenient to dynamically build
	/// up queries to plugins and send them off to the `target` plugin, in as few GRPC calls as
	/// possible
	pub fn batch<T>(&mut self, target: T) -> Result<QueryBuilder<'_>>
	where
		T: TryInto<QueryTarget, Error: Into<Error>>,
	{
		QueryBuilder::new(self, target)
	}

	/// Records a string-like concern that will be emitted in the final Hipcheck report.
	///
	/// "Concerns" should explain to users 1) what, specifically, a plugin has found that may be
	/// concerning about the target of analysis, 2) what the user may consider investigating next
	/// to better understand the potential risk identified.
	pub fn record_concern<S: AsRef<str>>(&mut self, concern: S) {
		fn inner(engine: &mut PluginEngine, concern: &str) {
			engine.concerns.push(concern.to_owned());
		}

		inner(self, concern.as_ref())
	}

	#[cfg(feature = "mock_engine")]
	#[cfg_attr(docsrs, doc(cfg(feature = "mock_engine")))]
	/// Exposes the current set of concerns recorded by `PluginEngine`.
	///
	/// This is only exposed for test code, where users may want to validate that their plugin
	/// produced the proper concerns given some mock query responses.
	pub fn get_concerns(&self) -> &[String] {
		&self.concerns
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
		let query_target = target.try_into().map_err(|e| e.into())?;

		tracing::trace!("querying {}", query_target.to_string());

		let input = serde_json::to_value(input)
			.map_err(|source| Error::InvalidJsonInQueryKey(Box::new(source)))?;

		let mut response = self.query_inner(query_target, vec![input]).await?;

		// PANIC SAFETY: since there input had one value, there will only be one response
		Ok(response.pop().unwrap())
	}

	/// Query another Hipcheck plugin `target` with Vec of `inputs`. On success, the JSONified
	/// result of the query is returned. `target` will often be a string of the format
	/// `"publisher/plugin[/query]"`, where the bracketed substring is optional if the plugin's
	/// default query endpoint is desired. `keys` must be a Vec containing a type which implements
	/// `serde::Serialize`,
	pub async fn batch_query<T, V>(&mut self, target: T, keys: Vec<V>) -> Result<Vec<JsonValue>>
	where
		T: TryInto<QueryTarget, Error: Into<Error>>,
		V: Serialize,
	{
		let target = target.try_into().map_err(|e| e.into())?;

		tracing::trace!("querying {}", target.to_string());

		let mut input = Vec::with_capacity(keys.len());

		for key in keys {
			let jsonified_key = serde_json::to_value(key)
				.map_err(|source| Error::InvalidJsonInQueryKey(Box::new(source)))?;

			input.push(jsonified_key);
		}

		self.query_inner(target, input).await
	}

	/// Helper function that actually performs the query send/recv.
	///
	/// Used by both `query` and `batch_query`.
	async fn query_inner(
		&mut self,
		target: QueryTarget,
		input: Vec<JsonValue>,
	) -> Result<Vec<JsonValue>> {
		// If we're mocking, use that instead.
		if cfg!(feature = "mock_engine") {
			return self.mock_query_inner(target, input).await;
		}

		// Send the query.
		self.send(Query {
			id: self.id,
			direction: QueryDirection::Request,
			publisher: target.publisher,
			plugin: target.plugin,
			query: target.query.unwrap_or_else(|| "".to_owned()),
			key: input,
			output: vec![],
			concerns: vec![],
		})
		.await?;

		// Match on the response.
		match self.recv().await? {
			Some(response) => Ok(response.output),
			None => Err(Error::SessionChannelClosed),
		}
	}

	async fn mock_query_inner(
		&mut self,
		target: QueryTarget,
		input: Vec<JsonValue>,
	) -> Result<Vec<JsonValue>> {
		let mut results = Vec::with_capacity(input.len());

		for i in input {
			// If doing a mock engine, look to the `mock_responses` field for the query answer
			match self.mock_responses.0.get(&(target.clone(), i)) {
				Some(res) => match res {
					Ok(val) => results.push(val.clone()),
					Err(e) => {
						tracing::error!("Error parsing mock_engine response: {e}");
						return Err(Error::UnexpectedPluginQueryInputFormat);
					}
				},
				None => {
					return Err(Error::UnknownPluginQuery(
						target.to_string().into_boxed_str(),
					));
				}
			}
		}

		Ok(results)
	}

	/// Send a query from plugin to the hipcheck server.
	///
	/// A single logical query may need to be "chunked" into multiple underlying gRPC messages.
	/// This `prepare` method from `hipcheck_common` is responsible for implementing chunking.
	/// It's in `common` because Hipcheck core (the `hc` binary) needs to do the same thing,
	/// and it's better if we share code.
	async fn send(&self, query: Query) -> Result<()> {
		let queries = hipcheck_common::chunk::prepare(query)?;

		for pq in queries {
			self.tx
				.send(Ok(InitiateQueryProtocolResponse { query: Some(pq) }))
				.await
				.map_err(|source| Error::FailedToSendQueryFromSessionToServer(Box::new(source)))?;
		}

		Ok(())
	}

	/// Receive a response back from Hipcheck core for the most recent query.
	async fn recv(&mut self) -> Result<Option<Query>> {
		let mut synth = QuerySynthesizer::default();

		let mut res = None;

		while res.is_none() {
			let Some(msg_chunks) = self.recv_chunks().await? else {
				return Ok(None);
			};

			res = synth.add(msg_chunks.into_iter())?;
		}

		Ok(res)
	}

	/// Receive chunks back from Hipcheck core.
	async fn recv_chunks(&mut self) -> Result<Option<VecDeque<PluginQuery>>> {
		let mut out = VecDeque::new();

		tracing::trace!("SDK: awaiting raw rx recv");

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
					tracing::warn!(
						"None received, gRPC channel closed. we may not close properly if None is not returned again"
					);
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

	pub(crate) async fn handle_session<P>(&mut self, plugin: Arc<P>)
	where
		P: Plugin,
	{
		if let Err(e) = self.handle_session_fallible(plugin).await {
			let res_err_send = match e {
				Error::FailedToSendQueryFromSessionToServer(_) => {
					tracing::error!("Failed to send message to Hipcheck core, analysis will hang.");
					return;
				}
				other => {
					tracing::error!("{}", other);
					self.send_session_err::<P>().await
				}
			};
			if res_err_send.is_err() {
				tracing::error!("Failed to send message to Hipcheck core, analysis will hang.");
			}
		}
	}

	async fn handle_session_fallible<P>(&mut self, plugin: Arc<P>) -> crate::error::Result<()>
	where
		P: Plugin,
	{
		let Some(query) = self.recv().await? else {
			return Err(Error::SessionChannelClosed);
		};

		if query.direction == QueryDirection::Response {
			return Err(Error::ReceivedReplyWhenExpectingRequest);
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
			.ok_or_else(|| {
				if name.is_empty() {
					Error::NoDefaultQuery
				} else {
					Error::UnknownPluginQuery(name.clone().into_boxed_str())
				}
			})?;

		#[cfg(feature = "print-timings")]
		let _0 = crate::benchmarking::print_scope_time!(format!("{}/{}", P::NAME, name));

		let value = query.run(self, key).await?;

		#[cfg(feature = "print-timings")]
		drop(_0);

		let query = Query {
			id: self.id,
			direction: QueryDirection::Response,
			publisher: P::PUBLISHER.to_owned(),
			plugin: P::NAME.to_owned(),
			query: name.to_owned(),
			key: vec![],
			output: vec![value],
			concerns: self.concerns.drain(..).collect(),
		};

		self.send(query).await
	}

	async fn send_session_err<P>(&mut self) -> crate::error::Result<()>
	where
		P: Plugin,
	{
		let query = proto::Query {
			id: self.id,
			state: QueryState::Unspecified as i32,
			publisher_name: P::PUBLISHER.to_owned(),
			plugin_name: P::NAME.to_owned(),
			query_name: "".to_owned(),
			key: vec![],
			output: vec![],
			concern: self.concerns.drain(..).collect(),
			split: false,
		};

		self.tx
			.send(Ok(InitiateQueryProtocolResponse { query: Some(query) }))
			.await
			.map_err(|source| Error::FailedToSendQueryFromSessionToServer(Box::new(source)))
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
			while let Err(e) = self.drop_tx.try_send(self.id) {
				match e {
					TrySendError::Closed(_) => break,
					// Retry if the queue is full; when it empties we'll drop.
					TrySendError::Full(_) => (),
				}
			}
		}
	}
}
