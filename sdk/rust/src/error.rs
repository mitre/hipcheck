// SPDX-License-Identifier: Apache-2.0

use hipcheck_common::proto::{
	ConfigurationStatus, InitiateQueryProtocolResponse, SetConfigurationResponse,
};
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

impl From<hipcheck_common::error::Error> for Error {
	fn from(value: hipcheck_common::error::Error) -> Self {
		use hipcheck_common::error::Error::*;
		match value {
			UnspecifiedQueryState => Error::UnspecifiedQueryState,
			UnexpectedRequestInProgress => Error::UnexpectedReplyInProgress,
			UnexpectedReplyInProgress => Error::UnexpectedReplyInProgress,
			ReceivedSubmitWhenExpectingReplyChunk => Error::ReceivedSubmitWhenExpectingReplyChunk,
			ReceivedReplyWhenExpectingSubmitChunk => Error::ReceivedReplyWhenExpectingRequest,
			MoreAfterQueryComplete { id } => Error::MoreAfterQueryComplete { id },
			InvalidJsonInQueryKey(s) => Error::InvalidJsonInQueryKey(s),
			InvalidJsonInQueryOutput(s) => Error::InvalidJsonInQueryOutput(s),
		}
	}
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

/// A Result type using `hipcheck_sdk::Error`
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

	/// The plugin encountered an error, probably due to incorrect assumptions.
	InternalError { message: String },

	/// A necessary plugin input file was not found.
	FileNotFound { file_path: String },

	/// The plugin's input data could not be parsed correctly.
	ParseError {
		// A short name or description of the data source.
		source: String,
		message: String,
	},

	/// An environment variable needed by the plugin was not set.
	EnvVarNotSet {
		/// Name of the environment variable
		env_var_name: String,
		/// Config field that set the variable name
		field_name: String,
		/// Message describing what the environment variable should contain
		purpose: String,
	},

	/// The plugin could not run a needed program.
	MissingProgram { program_name: String },
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
				message,
			},
			ConfigError::InternalError { message } => SetConfigurationResponse {
				status: ConfigurationStatus::InternalError as i32,
				message: format!("The plugin encountered an error, probably due to incorrect assumptions: {message}"),
			},
			ConfigError::FileNotFound { file_path } => SetConfigurationResponse {
				status: ConfigurationStatus::FileNotFound as i32,
				message: format!("File not found at path {file_path}"),
			},
			ConfigError::ParseError {
				source,
				message,
			} => SetConfigurationResponse {
				status: ConfigurationStatus::ParseError as i32,
				message: format!("The plugin's data from \"{source}\" could not be parsed correctly: {message}"),
			},
			ConfigError::EnvVarNotSet {
				env_var_name,
				field_name,
				purpose,
			} => SetConfigurationResponse {
				status: ConfigurationStatus::EnvVarNotSet as i32,
				message: format!("Could not find an environment variable with the name \"{env_var_name}\" (set by config field '{field_name}'). Purpose: {purpose}"),
			},
			ConfigError::MissingProgram { program_name } => SetConfigurationResponse {
				status: ConfigurationStatus::MissingProgram as i32,
				message: format!("The plugin could not find or run a needed program: {program_name}"),
			},
		}
	}
}
