// SPDX-License-Identifier: Apache-2.0

use hipcheck_sdk::prelude::*;
use serde::Deserialize;
use std::{result::Result as StdResult, sync::OnceLock};

pub struct GlobalConfig {
	config: OnceLock<Config>,
}

impl GlobalConfig {
	const fn new() -> Self {
		Self {
			config: OnceLock::new(),
		}
	}

	pub fn set(&self, config: serde_json::Value) -> StdResult<(), ConfigError> {
		let config = Config::try_from(config)?;
		self.config
			.set(config)
			.map_err(|_e| ConfigError::InternalError {
				message: "config was already set".to_owned().into_boxed_str(),
			})
	}

	pub fn api_token(&self) -> Result<&str> {
		Ok(self
			.config
			.get()
			.ok_or_else(|| {
				tracing::error!("tried to access config before set by Hipcheck core!");
				Error::UnspecifiedQueryState
			})?
			.api_token
			.as_str())
	}
}

/// Global configuration object.
///
/// Configuration is set only once, and is then read in many spots, so we make
/// it a static item instead of passing it around.
pub static CONFIG: GlobalConfig = GlobalConfig::new();

/// Validated configuration.
pub struct Config {
	/// The token to use for accessing the GitHub APIs.
	api_token: String,
}

impl TryFrom<serde_json::Value> for Config {
	type Error = ConfigError;

	fn try_from(value: serde_json::Value) -> StdResult<Self, Self::Error> {
		let raw: RawConfig =
			serde_json::from_value(value).map_err(|e| ConfigError::Unspecified {
				message: e.to_string().into_boxed_str(),
			})?;

		Self::try_from(raw)
	}
}

impl TryFrom<RawConfig> for Config {
	type Error = ConfigError;

	fn try_from(value: RawConfig) -> StdResult<Config, ConfigError> {
		let Some(atv) = value.api_token_var else {
			return Err(ConfigError::MissingRequiredConfig {
				field_name: "api-token-var".to_owned().into_boxed_str(),
				field_type: "name of environment variable containing GitHub API token"
					.to_owned()
					.into_boxed_str(),
				possible_values: vec![],
			});
		};

		let api_token = std::env::var(&atv).map_err(|_e| ConfigError::EnvVarNotSet {
			env_var_name: atv.clone().into_boxed_str(),
			purpose: "This environment variable must contain a GitHub API token."
				.to_owned()
				.into_boxed_str(),
		})?;

		Ok(Config { api_token })
	}
}

/// Raw configuration as read off the wire.
#[derive(Deserialize)]
pub struct RawConfig {
	#[serde(rename = "api-token-var")]
	api_token_var: Option<String>,
}
