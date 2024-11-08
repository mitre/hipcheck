// SPDX-License-Identifier: Apache-2.0

use crate::proto::{ConfigurationStatus, InitiateQueryProtocolResponse, SetConfigurationResponse};
use std::{convert::Infallible, error::Error as StdError, ops::Not, result::Result as StdResult};
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
		#[source] TokioMpscSendError<StdResult<InitiateQueryProtocolResponse, TonicStatus>>,
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

	#[error(transparent)]
	Unspecified { source: DynError },
}

impl From<anyhow::Error> for Error {
	fn from(value: anyhow::Error) -> Self {
		Error::Unspecified {
			source: value.into(),
		}
	}
}

impl Error {
	pub fn any<E: StdError + 'static + Send + Sync>(source: E) -> Self {
		Error::Unspecified {
			source: Box::new(source),
		}
	}
}

/// A thread-safe error trait object.
pub type DynError = Box<dyn StdError + 'static + Send + Sync>;

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

impl From<ConfigError> for SetConfigurationResponse {
	fn from(value: ConfigError) -> Self {
		match value {
			ConfigError::InvalidConfigValue {
				field_name,
				value,
				reason,
			} => SetConfigurationResponse {
				status: ConfigurationStatus::InvalidConfigurationValue as i32,
				message: format!("invalid value '{value}' for '{field_name}', reason: '{reason}'"),
			},
			ConfigError::MissingRequiredConfig {
				field_name,
				field_type,
				possible_values,
			} => SetConfigurationResponse {
				status: ConfigurationStatus::MissingRequiredConfiguration as i32,
				message: {
					let mut message = format!(
						"missing required config item '{field_name}' of type '{field_type}'"
					);

					if possible_values.is_empty().not() {
						message.push_str("; possible values: ");
						message.push_str(&possible_values.join(", "));
					}

					message
				},
			},
			ConfigError::UnrecognizedConfig {
				field_name,
				field_value,
				possible_confusables,
			} => SetConfigurationResponse {
				status: ConfigurationStatus::UnrecognizedConfiguration as i32,
				message: {
					let mut message =
						format!("unrecognized field '{field_name}' with value '{field_value}'");

					if possible_confusables.is_empty().not() {
						message.push_str("; possible field names: ");
						message.push_str(&possible_confusables.join(", "));
					}

					message
				},
			},
			ConfigError::Unspecified { message } => SetConfigurationResponse {
				status: ConfigurationStatus::Unspecified as i32,
				message: format!("unknown error; {message}"),
			},
		}
	}
}
