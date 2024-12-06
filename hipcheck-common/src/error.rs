// SPDX-License-Identifier: Apache-2.0

/// An enumeration of errors that can occur in a Hipcheck plugin
#[derive(Debug, thiserror::Error)]
pub enum Error {
	/// An unknown error occurred, the query is in an unspecified state
	#[error("unknown error; query is in an unspecified state")]
	UnspecifiedQueryState,

	/// The `PluginEngine` received a message with the unexpected status `ReplyInProgress`
	#[error("unexpected ReplyInProgress state for query")]
	UnexpectedReplyInProgress,

	/// The `PluginEngine` received a message with a request-type status when it expected a reply
	#[error("remote sent QuerySubmit when reply chunk expected")]
	ReceivedSubmitWhenExpectingReplyChunk,

	/// The `PluginEngine` received additional messages when it did not expect any
	#[error("received additional message for ID '{id}' after query completion")]
	MoreAfterQueryComplete { id: usize },

	#[error("invalid JSON in query key")]
	InvalidJsonInQueryKey(#[source] serde_json::Error),

	#[error("invalid JSON in query output")]
	InvalidJsonInQueryOutput(#[source] serde_json::Error),
}
