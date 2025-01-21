// SPDX-License-Identifier: Apache-2.0

use crate::{
	hc_error,
	policy_exprs::{std_parse, Expr},
	Result,
};
use futures::{Stream, StreamExt};
use hipcheck_common::proto::{
	plugin_service_client::PluginServiceClient, ConfigurationStatus, Empty,
	ExplainDefaultQueryRequest, GetDefaultPolicyExpressionRequest, GetQuerySchemasRequest,
	GetQuerySchemasResponse as PluginSchema, InitiateQueryProtocolRequest, Query as PluginQuery,
	SetConfigurationRequest, SetConfigurationResponse as PluginConfigResult,
};
use hipcheck_common::{chunk::QuerySynthesizer, types::*};
use serde_json::Value;
use std::{
	collections::{HashMap, VecDeque},
	convert::TryFrom,
	future::poll_fn,
	ops::Not as _,
	path::PathBuf,
	pin::Pin,
	process::Child,
	result::Result as StdResult,
};
use tokio::sync::{mpsc, Mutex};
use tokio_stream::wrappers::ReceiverStream;
use tonic::{transport::Channel, Code, Status};

pub type HcPluginClient = PluginServiceClient<Channel>;

#[derive(Clone, Debug)]
pub struct Plugin {
	pub name: String,
	pub working_dir: PathBuf,
	pub entrypoint: String,
}

// Hipcheck-facing version of struct from crate::hipcheck
#[derive(Clone, Debug)]
pub struct Schema {
	pub query_name: String,
	pub key_schema: Value,
	pub output_schema: Value,
}

impl TryFrom<PluginSchema> for Schema {
	type Error = crate::error::Error;
	fn try_from(value: PluginSchema) -> Result<Self> {
		let key_schema: Value = serde_json::from_str(value.key_schema.as_str())?;
		let output_schema: Value = serde_json::from_str(value.output_schema.as_str())?;
		Ok(Schema {
			query_name: value.query_name,
			key_schema,
			output_schema,
		})
	}
}

// Hipcheck-facing version of struct from crate::hipcheck
pub struct ConfigurationResult {
	pub status: ConfigurationStatus,
	pub message: Option<String>,
}

impl TryFrom<PluginConfigResult> for ConfigurationResult {
	type Error = crate::error::Error;
	fn try_from(value: PluginConfigResult) -> Result<Self> {
		let status: ConfigurationStatus = value.status.try_into()?;
		let message = value.message.is_empty().not().then_some(value.message);
		Ok(ConfigurationResult { status, message })
	}
}

// hipcheck::ConfigurationStatus has an enum that captures both error and success
// scenarios. The below code allows interpreting the struct as a Rust Result. If
// the success variant was the status, Ok(()) is returned, otherwise the code
// is stuffed into a custom error type enum that equals the protoc-generated one
// minus the success variant.
impl ConfigurationResult {
	pub fn as_result(&self) -> Result<()> {
		let Ok(error) = self.status.try_into() else {
			return Ok(());
		};
		Err(hc_error!(
			"{}",
			ConfigError::new(error, self.message.clone()).to_string()
		))
	}
}

pub enum ConfigErrorType {
	Unknown = 0,
	MissingRequiredConfig = 2,
	UnrecognizedConfig = 3,
	InvalidConfigValue = 4,
}

impl TryFrom<ConfigurationStatus> for ConfigErrorType {
	type Error = crate::error::Error;
	fn try_from(value: ConfigurationStatus) -> Result<Self> {
		use ConfigErrorType::*;
		use ConfigurationStatus::*;
		Ok(match value as i32 {
			x if x == Unspecified as i32 => Unknown,
			x if x == MissingRequiredConfiguration as i32 => MissingRequiredConfig,
			x if x == UnrecognizedConfiguration as i32 => UnrecognizedConfig,
			x if x == InvalidConfigurationValue as i32 => InvalidConfigValue,
			x => {
				return Err(hc_error!("status value '{}' is not an error", x));
			}
		})
	}
}

pub struct ConfigError {
	error: ConfigErrorType,
	message: Option<String>,
}

impl ConfigError {
	pub fn new(error: ConfigErrorType, message: Option<String>) -> Self {
		ConfigError { error, message }
	}
}

impl std::fmt::Display for ConfigError {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> StdResult<(), std::fmt::Error> {
		use ConfigErrorType::*;
		let msg = match &self.message {
			Some(s) => format!(": {s}"),
			None => "".to_owned(),
		};
		let err = match self.error {
			Unknown => "unknown configuration error occurred",
			MissingRequiredConfig => "configuration is missing required fields",
			UnrecognizedConfig => "configuration contains unrecognized fields",
			InvalidConfigValue => "configuration contains invalid values",
		};
		write!(f, "{}{}", msg, err)
	}
}

/// State for managing an actively running plugin process
///
/// Note that `PluginContext` is basically a builder for `PluginTransport`, which
/// runs the startup RPCs, launches the query protocol, and then hands off management
/// of that protocol to `PluginTransport`.
#[derive(Debug)]
pub struct PluginContext {
	/// The plugin being wrapped.
	pub plugin: Plugin,

	/// The port that plugin is listening on.
	pub port: u16,

	/// A gRPC client for interacting with the plugin.
	pub grpc: HcPluginClient,

	/// The child process in which the plugin is running.
	pub proc: Child,

	/// The size of the gRPC buffer
	pub grpc_query_buffer_size: usize,
}

// Redefinition of `grpc` field's functions with more useful types, additional
// error & sanity checking
impl PluginContext {
	/// Get schemas for all queries supported by the plugin.
	pub async fn get_query_schemas(&mut self) -> Result<Vec<Schema>> {
		let mut res = self
			.grpc
			.get_query_schemas(GetQuerySchemasRequest {
				empty: Some(Empty {}),
			})
			.await?;

		let mut schemas: HashMap<_, PluginSchema> = HashMap::new();
		while let Some(msg) = res.get_mut().message().await? {
			// If we received a PluginSchema msg with this query name before,
			// treat as a chunked msg and append its strings to existing entry
			schemas
				.entry(msg.query_name.clone())
				.and_modify(|existing| {
					existing.key_schema.push_str(&msg.key_schema);
					existing.output_schema.push_str(&msg.output_schema);
				})
				.or_insert(msg);
		}

		// Convert the aggregated PluginSchemas to Schema objects
		schemas.into_values().map(TryInto::try_into).collect()
	}

	/// Set configuration on the plugin.
	///
	/// Plugins are expected to do error handling on their side for the various ways that
	/// configuration may be wrong, and we report that if configuration is wrong.
	pub async fn set_configuration(&mut self, conf: &Value) -> Result<ConfigurationResult> {
		self.grpc
			.set_configuration(SetConfigurationRequest {
				configuration: serde_json::to_string(&conf)?,
			})
			.await?
			.into_inner()
			.try_into()
	}

	/// Get the default policy expression from a plugin, if one is defined.
	pub async fn get_default_policy_expression(&mut self) -> Result<Option<String>> {
		let req = GetDefaultPolicyExpressionRequest {
			empty: Some(Empty {}),
		};

		let res = self.grpc.get_default_policy_expression(req).await?;
		let expression = &res.get_ref().policy_expression;

		if expression.is_empty() {
			Ok(None)
		} else {
			Ok(Some(expression.clone()))
		}
	}

	/// Get an explanation of the default query, to use when reporting results.
	pub async fn explain_default_query(&mut self) -> Result<Option<String>> {
		let req = ExplainDefaultQueryRequest {
			empty: Some(Empty {}),
		};

		let res = &self.grpc.explain_default_query(req).await?;
		let explanation = &res.get_ref().explanation;

		if explanation.is_empty() {
			Ok(None)
		} else {
			Ok(Some(explanation.clone()))
		}
	}

	/// Initiate the query protocol.
	///
	/// This is the most complex RPC call by far, as it initiates a bidirectional
	/// streaming RPC in which we run our "query protocol" as defined in RFD #4.
	pub async fn initiate_query_protocol(
		&mut self,
		rx: mpsc::Receiver<PluginQuery>,
	) -> Result<QueryStream> {
		// Convert the receiver into a stream.
		let stream = ReceiverStream::new(rx)
			.map(|query| InitiateQueryProtocolRequest { query: Some(query) });

		// Make the gRPC request.
		let resp = self
			.grpc
			.initiate_query_protocol(stream)
			.await
			.map_err(|err| {
				hc_error!(
					"query protocol initiation failed with tonic status code {}",
					err
				)
			})?;

		// Pull out the inner query from the response.
		let stream = resp.into_inner().map(|response| {
			response.and_then(|res| {
				res.query
					.ok_or_else(|| Status::new(Code::Unknown, "no query present in response"))
			})
		});

		Ok(Box::new(stream))
	}

	/// Consume the builder and run the query protocol.
	///
	/// Consume self and produce a `PluginTransport` which will handle
	/// execution of the query protocol over the still-open bidirectional
	/// `InitiateQueryProtocol` RPC.
	pub async fn initialize(mut self, config: Value) -> Result<PluginTransport> {
		// NOTE: The order of these operations is purposeful, and they should _not_
		// be re-ordered.

		let schemas = HashMap::from_iter(
			self.get_query_schemas()
				.await?
				.into_iter()
				.map(|schema| (schema.query_name.clone(), schema)),
		);

		self.set_configuration(&config).await?.as_result()?;

		let opt_str = self.get_default_policy_expression().await?;
		// This is where we turn the `std_parse` error into a user-facing message
		let opt_default_policy_expr = opt_str
			.map(|s| {
				std_parse(s.as_str()).map_err(|e| {
					hc_error!(
						"Plugin '{}' has bad default policy expression '{}': {}",
						self.plugin.name,
						s,
						e
					)
				})
			})
			.transpose()?;

		let opt_explain_default_query = self.explain_default_query().await?;

		let (tx, out_rx) = mpsc::channel::<PluginQuery>(self.grpc_query_buffer_size);
		let rx = self.initiate_query_protocol(out_rx).await?;

		Ok(PluginTransport {
			schemas,
			opt_default_policy_expr,
			opt_explain_default_query,
			ctx: self,
			tx,
			rx: Mutex::new(MultiplexedQueryReceiver::new(rx)),
		})
	}
}
impl Drop for PluginContext {
	fn drop(&mut self) {
		if let Err(e) = self.proc.kill() {
			println!("Failed to kill child: {e}");
		}
	}
}

pub struct MultiplexedQueryReceiver {
	rx: QueryStream,
	backlog: HashMap<i32, VecDeque<PluginQuery>>,
}

impl std::fmt::Debug for MultiplexedQueryReceiver {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("MultiplexedQueryReceiver")
			.field("rx", &"<receiver>")
			.field("backlog", &self.backlog)
			.finish()
	}
}

/// Helper type for a stream of query messages.
///
/// Note that the inner item is a `Result` because the inner
/// `query` field on the message can technically be missing.
///
/// This case is handled in the `message` method to flatten
/// that kind of error so code consuming the stream doesn't
/// have to worry about it.
type QueryStream = Box<dyn Stream<Item = StdResult<PluginQuery, Status>> + Send + Unpin + 'static>;

impl MultiplexedQueryReceiver {
	pub fn new(rx: QueryStream) -> Self {
		Self {
			rx,
			backlog: HashMap::new(),
		}
	}

	/// Poll the underlying stream future to get the next query, if present.
	async fn message(&mut self) -> StdResult<Option<PluginQuery>, Status> {
		match poll_fn(|cx| Pin::new(self.rx.as_mut()).poll_next(cx)).await {
			Some(Ok(m)) => Ok(Some(m)),
			Some(Err(e)) => Err(e),
			None => Ok(None),
		}
	}

	// @Invariant - this function will never return an empty VecDeque
	pub async fn recv(&mut self, id: i32) -> Result<Option<VecDeque<PluginQuery>>> {
		// If we have 1+ messages on backlog for `id`, return them all,
		// no need to waste time with successive calls
		if let Some(msgs) = self.backlog.remove(&id) {
			return Ok(Some(msgs));
		}

		// No backlog message, need to operate the receiver
		loop {
			let Some(raw) = self.message().await? else {
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

// Encapsulate an "initialized" state of a Plugin with interfaces that abstract
// query chunking to produce whole messages for the Hipcheck engine
#[derive(Debug)]
pub struct PluginTransport {
	pub schemas: HashMap<String, Schema>,
	pub opt_default_policy_expr: Option<Expr>,
	pub opt_explain_default_query: Option<String>,
	ctx: PluginContext,
	tx: mpsc::Sender<PluginQuery>,
	rx: Mutex<MultiplexedQueryReceiver>,
}

impl PluginTransport {
	pub fn name(&self) -> &str {
		&self.ctx.plugin.name
	}

	pub async fn query(&self, query: Query) -> Result<Option<Query>> {
		// Send the query
		let id = query.id as i32;
		let queries = hipcheck_common::chunk::prepare(query).map_err(|e| hc_error!("{}", e))?;

		for query in queries {
			self.tx
				.send(query)
				.await
				.map_err(|e| hc_error!("sending query failed: {}", e))?;
		}

		// De-chunk received messages into a response Query object
		let mut synth = QuerySynthesizer::default();
		let mut res: Option<Query> = None;
		while res.is_none() {
			// Get initial response batch
			let mut rx_handle = self.rx.lock().await;
			let Some(msg_chunks) = rx_handle.recv(id).await? else {
				return Ok(None);
			};
			drop(rx_handle);
			res = synth.add(msg_chunks.into_iter())?;
		}

		Ok(res)
	}
}

pub struct PluginWithConfig(pub Plugin, pub Value);
impl From<PluginWithConfig> for (Plugin, Value) {
	fn from(value: PluginWithConfig) -> Self {
		(value.0, value.1)
	}
}

pub struct PluginContextWithConfig(pub PluginContext, pub Value);
impl From<PluginContextWithConfig> for (PluginContext, Value) {
	fn from(value: PluginContextWithConfig) -> Self {
		(value.0, value.1)
	}
}

#[derive(Clone, Debug)]
pub struct AwaitingResult {
	pub id: usize,
	pub publisher: String,
	pub plugin: String,
	pub query: String,
	pub key: Vec<Value>,
}

impl From<Query> for AwaitingResult {
	fn from(value: Query) -> Self {
		AwaitingResult {
			id: value.id,
			publisher: value.publisher,
			plugin: value.plugin,
			query: value.query,
			key: value.key,
		}
	}
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryResult {
	pub value: Vec<Value>,
	pub concerns: Vec<String>,
}

#[derive(Clone, Debug)]
pub enum PluginResponse {
	RemoteClosed,
	AwaitingResult(AwaitingResult),
	Completed(QueryResult),
}

impl From<Option<Query>> for PluginResponse {
	fn from(value: Option<Query>) -> Self {
		match value {
			Some(q) => q.into(),
			None => PluginResponse::RemoteClosed,
		}
	}
}

impl From<Query> for PluginResponse {
	fn from(value: Query) -> Self {
		if value.direction == QueryDirection::Response {
			let result = QueryResult {
				value: value.output,
				concerns: value.concerns,
			};
			PluginResponse::Completed(result)
		} else {
			PluginResponse::AwaitingResult(value.into())
		}
	}
}

pub fn get_plugin_key(publisher: &str, plugin: &str) -> String {
	format!("{publisher}/{plugin}")
}
