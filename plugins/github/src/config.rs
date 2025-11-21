// SPDX-License-Identifier: Apache-2.0

use hipcheck_sdk::prelude::*;
use serde::Deserialize;
use std::{result::Result as StdResult, sync::OnceLock};

/// Global configuration object.
///
/// Configuration is set only once, and is then read in many spots, so we make
/// it a static item instead of passing it around.
pub static CONFIG: OnceLock<Config> = OnceLock::new();

/// Validated configuration.
pub struct Config {
	/// The token to use for accessing the GitHub APIs.
	pub api_token: String,
}

/// Raw configuration as read off the wire.
#[derive(Deserialize)]
pub struct RawConfig {
	#[serde(rename = "api-token-var")]
	api_token_var: Option<String>,
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
