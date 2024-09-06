// SPDX-License-Identifier: Apache-2.0

//! Data types and functions for parsing policy KDL files

pub mod config_to_policy;
pub mod policy_file;
mod tests;

use crate::kdl_helper::extract_data;
use crate::policy::policy_file::{PolicyAnalyze, PolicyPluginList};
use crate::util::fs as file;
use crate::{error::Result, hc_error};
use kdl::KdlDocument;
use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolicyFile {
	pub plugins: PolicyPluginList,
	pub analyze: PolicyAnalyze,
}

impl FromStr for PolicyFile {
	type Err = crate::Error;

	fn from_str(s: &str) -> Result<Self> {
		let document =
			KdlDocument::from_str(s).map_err(|e| hc_error!("Error parsing policy file: {}", e))?;
		let nodes = document.nodes();

		let plugins: PolicyPluginList =
			extract_data(nodes).ok_or_else(|| hc_error!("Could not parse 'plugins'"))?;
		let analyze: PolicyAnalyze =
			extract_data(nodes).ok_or_else(|| hc_error!("Could not parse 'analyze'"))?;

		Ok(Self { plugins, analyze })
	}
}

impl PolicyFile {
	/// Load policy from the given file.
	pub fn load_from(policy_path: &Path) -> Result<PolicyFile> {
		if policy_path.is_dir() {
			return Err(hc_error!(
				"Hipcheck policy path must be a file, not a directory."
			));
		}
		file::exists(policy_path)?;
		let policy = PolicyFile::from_str(&file::read_string(policy_path)?)?;

		Ok(policy)
	}

	/// Try to get the configuration for a specific analysis.
	///
	/// The idea here is that this recursively searches down the analysis tree to
	/// find a matching analysis, and then processes the config block into a
	/// `HashMap` that can be passed along to plugins during startup.
	///
	/// In the future we'd like to use an implementation of the KDL query language
	/// directly on the KDL data, I think, but no such implementation exists today,
	/// so this will have to do.
	#[allow(unused)]
	pub fn get_config(&self, analysis_name: &str) -> Option<HashMap<String, String>> {
		self.analyze
			.find_analysis_by_name(analysis_name)
			.map(|analysis| analysis.config.map(|config| config.0).unwrap_or_default())
	}
}
