use crate::hipcheck::plugin_client::PluginClient;
use crate::hipcheck::{
	Configuration, ConfigurationResult as PluginConfigResult, ConfigurationStatus, Empty,
	Query as PluginQuery, QueryState, Schema as PluginSchema,
};
use crate::{hc_error, Error, Result, StdResult};
use serde_json::Value;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::ops::Not;
use std::process::Child;
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
		mut rx: tokio::sync::mpsc::Receiver<PluginQuery>,
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
		let (tx, mut out_rx) = tokio::sync::mpsc::channel::<PluginQuery>(10);
		let rx = self.initiate_query_protocol(out_rx).await?;
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

struct Query {
	id: usize,
	// if false, response
	request: bool,
	publisher: String,
	plugin: String,
	query: String,
	key: Value,
	output: Value,
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

// Encapsulate an "initialized" state of a Plugin with interfaces that abstract
// query chunking to produce whole messages for the Hipcheck engine
pub struct PluginTransport {
	pub schemas: HashMap<String, Schema>,
	pub default_policy_expr: String, // TODO - update with policy_expr type
	ctx: PluginContext,
	tx: tokio::sync::mpsc::Sender<PluginQuery>,
	rx: Streaming<PluginQuery>,
}
impl PluginTransport {
	pub fn name(&self) -> &str {
		&self.ctx.plugin.name
	}
	async fn send(&mut self, query: Query) -> Result<()> {
		let query: PluginQuery = query.try_into()?;
		self.tx
			.send(query)
			.await
			.map_err(|e| hc_error!("sending query failed: {}", e))
	}
	async fn recv(&mut self) -> Result<Option<Query>> {
		use QueryState::*;
		let Some(mut raw) = self.rx.message().await? else {
			// gRPC channel was closed
			return Ok(None);
		};
		let mut state: QueryState = raw.state.try_into()?;
		// As long as we expect successive chunks, keep receiving
		if matches!(state, QueryReplyInProgress) {
			while matches!(state, QueryReplyInProgress) {
				println!("Retrieving next response");
				let Some(next) = self.rx.message().await? else {
					return Err(hc_error!(
						"plugin gRPC channel closed while sending chunked message"
					));
				};
				// Assert that the ids are consistent
				if next.id != raw.id {
					return Err(hc_error!("msg ids from plugin do not match"));
				}
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
