// SPDX-License-Identifier: Apache-2.0

use hipcheck_sdk::PluginConfig;
use hipcheck_sdk::error::ConfigError;
use hipcheck_sdk::macros::PluginConfig;
use std::path::PathBuf;
use std::result::Result as StdResult;

/// Configuration for the affiliation plugin.
#[derive(PluginConfig, Debug)]
pub struct Config {
	/// Path to the orgs file to use for defining matching behavior.
	pub orgs_file: PathBuf,

	/// The threshold of how many matches is too many.
	pub count_threshold: Option<u64>,
}
