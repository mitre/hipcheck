use crate::hipcheck::plugin_client::PluginClient;
use crate::hipcheck::{
	Configuration, ConfigurationResult as PluginConfigResult, ConfigurationStatus, Empty,
	Query as PluginQuery, QueryState, Schema as PluginSchema,
};
use crate::{hc_error, Error, Result, StdResult};
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::convert::TryFrom;
use std::ops::Not;
use std::process::Child;
use tokio::sync::{mpsc, Mutex};
use tonic::codec::Streaming;
use tonic::transport::Channel;

pub type HcPluginClient = PluginClient<Channel>;

#[derive(Clone, Debug)]
pub struct Plugin {
	pub name: String,
	pub entrypoint: String,
}

// Hipcheck-facing version of struct from crate::hipcheck
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
			x if x == ErrorUnknown as i32 => Unknown,
			x if x == ErrorMissingRequiredConfiguration as i32 => MissingRequiredConfig,
			x if x == ErrorUnrecognizedConfiguration as i32 => UnrecognizedConfig,
			x if x == ErrorInvalidConfigurationValue as i32 => InvalidConfigValue,
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
			MissingRequiredConfig => "configuration is missing requried fields",
			UnrecognizedConfig => "configuration contains unrecognized fields",
			InvalidConfigValue => "configuration contains invalid values",
		};
		write!(f, "{}{}", msg, err)
	}
}

// State for managing an actively running plugin process
pub struct PluginContext {
	pub plugin: Plugin,
	pub port: u16,
	pub grpc: HcPluginClient,
	pub proc: Child,
}
// Redefinition of `grpc` field's functions with more useful types, additional
// error & sanity checking
impl PluginContext {
	pub async fn get_query_schemas(&mut self) -> Result<Vec<Schema>> {
		let mut res = self.grpc.get_query_schemas(Empty {}).await?;
		let stream = res.get_mut();
		let mut schema_builder: HashMap<String, PluginSchema> = HashMap::new();
		while let Some(msg) = stream.message().await? {
			// If we received a PluginSchema msg with this query name before,
			// treat as a chunked msg and append its strings to existing entry
			if let Some(existing) = schema_builder.get_mut(&msg.query_name) {
				existing.key_schema.push_str(msg.key_schema.as_str());
				existing.output_schema.push_str(msg.output_schema.as_str());
			} else {
				schema_builder.insert(msg.query_name.clone(), msg);
			}
		}
		// Convert the aggregated PluginSchemas to Schema objects
		schema_builder
			.into_values()
			.map(TryInto::try_into)
			.collect()
	}
	pub async fn set_configuration(&mut self, conf: &Value) -> Result<ConfigurationResult> {
		let conf_query = Configuration {
			configuration: serde_json::to_string(&conf)?,
		};
		let res = self.grpc.set_configuration(conf_query).await?;
		res.into_inner().try_into()
	}
	// TODO - the String in the result should be replaced with a structured
	// type once the policy expression code is integrated
	pub async fn get_default_policy_expression(&mut self) -> Result<String> {
		let mut res = self.grpc.get_default_policy_expression(Empty {}).await?;
		Ok(res.get_ref().policy_expression.to_owned())
	}
	pub async fn initiate_query_protocol(
		&mut self,
		mut rx: mpsc::Receiver<PluginQuery>,
	) -> Result<Streaming<PluginQuery>> {
		let stream = async_stream::stream! {
			while let Some(item) = rx.recv().await {
				yield item;
			}
		};
		match self.grpc.initiate_query_protocol(stream).await {
			Ok(resp) => Ok(resp.into_inner()),
			Err(e) => Err(hc_error!(
				"query protocol initiation failed with tonic status code {}",
				e
			)),
		}
	}
	pub async fn initialize(mut self, config: Value) -> Result<PluginTransport> {
		let schemas = HashMap::<String, Schema>::from_iter(
			self.get_query_schemas()
				.await?
				.into_iter()
				.map(|s| (s.query_name.clone(), s)),
		);
		self.set_configuration(&config).await?.as_result()?;
		let default_policy_expr = self.get_default_policy_expression().await?;
		let (tx, mut out_rx) = mpsc::channel::<PluginQuery>(10);
		let rx = self.initiate_query_protocol(out_rx).await?;
		let rx = Mutex::new(MultiplexedQueryReceiver::new(rx));
		Ok(PluginTransport {
			schemas,
			default_policy_expr,
			ctx: self,
			tx,
			rx,
			//	active_query: None,
			//	last_id: 0,
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

pub enum HcQueryResult {
	Ok(Value),
	Needs(String, String, String, Value),
	Err(Error),
}

#[derive(Debug)]
pub struct Query {
	pub id: usize,
	// if false, response
	pub request: bool,
	pub publisher: String,
	pub plugin: String,
	pub query: String,
	pub key: Value,
	pub output: Value,
}
impl TryFrom<PluginQuery> for Query {
	type Error = Error;
	fn try_from(value: PluginQuery) -> Result<Query> {
		use QueryState::*;
		let request = match TryInto::<QueryState>::try_into(value.state)? {
			QueryUnspecified => return Err(hc_error!("unspecified error from plugin")),
			QueryReplyInProgress => {
				return Err(hc_error!(
					"invalid state QueryReplyInProgress for conversion to Query"
				))
			}
			QueryReplyComplete => false,
			QuerySubmit => true,
		};
		let key: Value = serde_json::from_str(value.key.as_str())?;
		let output: Value = serde_json::from_str(value.output.as_str())?;
		Ok(Query {
			id: value.id as usize,
			request,
			publisher: value.publisher_name,
			plugin: value.plugin_name,
			query: value.query_name,
			key,
			output,
		})
	}
}
impl TryFrom<Query> for PluginQuery {
	type Error = crate::error::Error;
	fn try_from(value: Query) -> Result<PluginQuery> {
		let state_enum = match value.request {
			true => QueryState::QuerySubmit,
			false => QueryState::QueryReplyComplete,
		};
		let key = serde_json::to_string(&value.key)?;
		let output = serde_json::to_string(&value.output)?;
		Ok(PluginQuery {
			id: value.id as i32,
			state: state_enum as i32,
			publisher_name: value.publisher,
			plugin_name: value.plugin,
			query_name: value.query,
			key,
			output,
		})
	}
}

pub struct MultiplexedQueryReceiver {
	rx: Streaming<PluginQuery>,
	backlog: HashMap<i32, VecDeque<PluginQuery>>,
}
impl MultiplexedQueryReceiver {
	pub fn new(rx: Streaming<PluginQuery>) -> Self {
		Self {
			rx,
			backlog: HashMap::new(),
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
			let Some(raw) = self.rx.message().await? else {
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
pub struct PluginTransport {
	pub schemas: HashMap<String, Schema>,
	pub default_policy_expr: String, // TODO - update with policy_expr type
	ctx: PluginContext,
	tx: mpsc::Sender<PluginQuery>,
	rx: Mutex<MultiplexedQueryReceiver>,
}
impl PluginTransport {
	pub fn name(&self) -> &str {
		&self.ctx.plugin.name
	}
	pub async fn query(&self, query: Query) -> Result<Option<Query>> {
		use QueryState::*;

		// Send the query
		let query: PluginQuery = query.try_into()?;
		let id = query.id;
		self.tx
			.send(query)
			.await
			.map_err(|e| hc_error!("sending query failed: {}", e))?;

		// Get initial response batch
		let mut rx_handle = self.rx.lock().await;
		let Some(mut msg_chunks) = rx_handle.recv(id).await? else {
			return Ok(None);
		};
		drop(rx_handle);

		let mut raw = msg_chunks.pop_front().unwrap();
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
						let mut rx_handle = self.rx.lock().await;
						match rx_handle.recv(id).await? {
							Some(x) => {
								drop(rx_handle);
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
					QueryUnspecified => return Err(hc_error!("unspecified error from plugin")),
					QuerySubmit => {
						return Err(hc_error!(
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
				return Err(hc_error!(
					"received additional messages for id '{}' after QueryComplete status message",
					id
				));
			}
		}
		raw.try_into().map(Some)
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
	pub key: Value,
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

#[derive(Clone, Debug)]
pub enum PluginResponse {
	RemoteClosed,
	AwaitingResult(AwaitingResult),
	Completed(Value),
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
		if !value.request {
			PluginResponse::Completed(value.output)
		} else {
			PluginResponse::AwaitingResult(value.into())
		}
	}
}
