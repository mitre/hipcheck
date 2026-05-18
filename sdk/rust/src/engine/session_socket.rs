// SPDX-License-Identifier: Apache-2.0

use crate::{
	engine::{PluginEngine, mock_responses::MockResponses},
	error::{Error, Result},
	plugin::Plugin,
};
use futures::Stream;
use hipcheck_common::proto::{
	InitiateQueryProtocolRequest, InitiateQueryProtocolResponse, Query as PluginQuery, QueryState,
};
use std::{
	collections::HashMap, future::poll_fn, pin::Pin, result::Result as StdResult, sync::Arc,
};
use tokio::sync::mpsc;
use tonic::Status;

/// Map from session IDs to query senders.
type SessionTracker = HashMap<i32, mpsc::Sender<Option<PluginQuery>>>;

/// A stream of queries.
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
				Some(_) => tracing::trace!("Cleaned up session {id}"),
				None => {
					tracing::warn!(
						"HcSessionSocket got request to drop a session that does not exist"
					)
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
					tracing::trace!("SDK: forwarding message to session {id}");

					if let Err(_e) = tx.send(Some(raw)).await {
						tracing::error!("Error forwarding msg to session {id}");
						self.sessions.remove(&id);
					};
				}
				Ok(HandleAction::CreateSession) => {
					tracing::trace!("SDK: creating new session {id}");

					let (in_tx, rx) = mpsc::channel::<Option<PluginQuery>>(10);
					let tx = self.tx.clone();

					let session = PluginEngine {
						id,
						concerns: vec![],
						tx,
						rx,
						drop_tx: self.drop_tx.clone(),
						mock_responses: MockResponses::new(),
					};

					in_tx.send(Some(raw)).await.expect(
						"Failed sending message to newly created Session, should never happen",
					);

					tracing::trace!("SDK: adding new session {id} to tracker");
					self.sessions.insert(id, in_tx);

					return Ok(Some(session));
				}
				Err(e) => tracing::error!("{}", e),
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
				tracing::trace!("Channel closed by remote");
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

pub(crate) enum HandleAction<'s> {
	ForwardMsgToExistingSession(&'s mut mpsc::Sender<Option<PluginQuery>>),
	CreateSession,
}
