use crate::error::ConfigError;
use std::result::Result as StdResult;

/// The trait used to deserialized plugin config input from the Policy File.
/// The trait is applied to a plugin RawConfig struct and works in tandem with
/// the derive_plugin_config procedural macro re-imported to this sdk crate
/// via hipcheck_sdk_macros.
pub trait PluginConfig<'de> {
	fn deserialize(config: &serde_json::Value) -> StdResult<Self, ConfigError>
	where
		Self: Sized;
}
