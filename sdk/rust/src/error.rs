// SPDX-License-Identifier: Apache-2.0

use crate::proto::QueryResponse;
use std::{convert::Infallible, result::Result as StdResult};
use tokio::sync::mpsc::error::SendError as TokioMpscSendError;
use tonic::Status as TonicStatus;

/// An enumeration of errors that can occur in a Hipcheck plugin
#[derive(Debug, thiserror::Error)]
pub enum Error {
	/// An unknown error occurred, the query is in an unspecified state
	#[error("unknown error; query is in an unspecified state")]
	UnspecifiedQueryState,

	/// The `PluginEngine` received a message with the unexpected status `ReplyInProgress`
	#[error("unexpected ReplyInProgress state for query")]
	UnexpectedReplyInProgress,

	#[error("invalid JSON in query key")]
	InvalidJsonInQueryKey(#[source] serde_json::Error),

	#[error("invalid JSON in query output")]
	InvalidJsonInQueryOutput(#[source] serde_json::Error),

	#[error("session channel closed unexpectedly")]
	SessionChannelClosed,

	#[error("failed to send query from session to server")]
	FailedToSendQueryFromSessionToServer(
		#[source] TokioMpscSendError<StdResult<QueryResponse, TonicStatus>>,
	),

	/// The `PluginEngine` received a message with a reply-type status when it expected a request
	#[error("plugin sent QueryReply when server was expecting a request")]
	ReceivedReplyWhenExpectingRequest,

	/// The `PluginEngine` received a message with a request-type status when it expected a reply
	#[error("plugin sent QuerySubmit when server was expecting a reply chunk")]
	ReceivedSubmitWhenExpectingReplyChunk,

	/// The `PluginEngine` received additional messages when it did not expect any
	#[error("received additional message for ID '{id}' after query completion")]
	MoreAfterQueryComplete { id: usize },

	#[error("failed to start server")]
	FailedToStartServer(#[source] tonic::transport::Error),

	/// The `Query::run` function implementation received an incorrectly-typed JSON Value key
	#[error("unexpected JSON value from plugin")]
	UnexpectedPluginQueryInputFormat,

	/// The `Query::run` function implementation produced an output that cannot be serialized to JSON
	#[error("plugin output could not be serialized to JSON")]
	UnexpectedPluginQueryOutputFormat,

	/// The `PluginEngine` received a request for an unknown query endpoint
	#[error("could not determine which plugin query to run")]
	UnknownPluginQuery,

	#[error("invalid format for QueryTarget")]
	InvalidQueryTargetFormat,
}

// this will never happen, but is needed to enable passing QueryTarget to PluginEngine::query
impl From<Infallible> for Error {
	fn from(_value: Infallible) -> Self {
		Error::UnspecifiedQueryState
	}
}

pub type Result<T> = StdResult<T, Error>;

/// Errors specific to the execution of `Plugin::set_configuration()` to configure a Hipcheck
/// plugin.
#[derive(Debug)]
pub enum ConfigError {
	/// The config key was valid, but the associated value was invalid
	InvalidConfigValue {
		field_name: String,
		value: String,
		reason: String,
	},

	/// The config was missing an expected field
	MissingRequiredConfig {
		field_name: String,
		field_type: String,
		possible_values: Vec<String>,
	},

	/// The config included an unrecognized field
	UnrecognizedConfig {
		field_name: String,
		field_value: String,
		possible_confusables: Vec<String>,
	},

	/// An unspecified error
	Unspecified { message: String },
}

impl From<ConfigError> for TonicStatus {
	fn from(val: ConfigError) -> Self {
		match val {
			ConfigError::InvalidConfigValue {
				field_name,
				value,
				reason,
			} => {
				let msg = format!(
					"unknown '{}' config value '{}', reason: {}",
					field_name, value, reason
				);
				TonicStatus::invalid_argument(msg)
			}
			ConfigError::MissingRequiredConfig {
				field_name,
				field_type,
				possible_values,
			} => {
				let msg = format!(
					"missing required config item '{}' of type '{}', possible values: {}",
					field_name,
					field_type,
					possible_values.join(", ")
				);
				TonicStatus::invalid_argument(msg)
			}
			ConfigError::UnrecognizedConfig {
				field_name,
				field_value,
				possible_confusables,
			} => {
				let msg = format!(
					"unrecognized config item '{}' with value '{}', did you mean one of: {}",
					field_name,
					field_value,
					possible_confusables.join(", ")
				);
				TonicStatus::invalid_argument(msg)
			}
			ConfigError::Unspecified { message } => TonicStatus::unknown(message),
		}
	}
}
