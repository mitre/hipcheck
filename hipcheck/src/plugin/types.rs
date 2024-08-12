use crate::hipcheck::plugin_client::PluginClient;
use crate::hipcheck::{
	Configuration, ConfigurationResult as PluginConfigResult, ConfigurationStatus, Empty,
	Schema as PluginSchema,
};
use crate::{hc_error, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::ops::Not;
use std::process::Child;
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
	pub fn as_result(&self) -> std::result::Result<(), ConfigError> {
		let Ok(error) = self.status.try_into() else {
			return Ok(());
		};
		Err(ConfigError::new(error, self.message.clone()))
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

// State for managing an actively running plugin process
pub struct PluginContext {
	pub plugin: Plugin,
	pub port: u16,
	pub grpc: HcPluginClient,
	pub proc: Child,
	pub channel: Option<String>,
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
}
impl Drop for PluginContext {
	fn drop(&mut self) {
		if let Err(e) = self.proc.kill() {
			println!("Failed to kill child: {e}");
		}
	}
}
