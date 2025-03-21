// SPDX-License-Identifier: Apache-2.0

//! Data types and functions for parsing policy KDL files

mod config_to_policy;
mod macros;
pub mod policy_file;
mod tests;

pub use config_to_policy::config_to_policy;

use crate::{
	error::Result,
	hc_error,
	policy::policy_file::{PolicyAnalyze, PolicyPatchList, PolicyPluginList, PolicyPluginName},
	policy_exprs::{std_parse, Expr},
	util::fs as file,
};
use hipcheck_kdl::extract_data;
use hipcheck_kdl::kdl::KdlDocument;
use miette::Report;
use serde_json::Value;
use std::{collections::HashMap, path::Path, str::FromStr};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolicyFile {
	pub plugins: PolicyPluginList,
	pub patch: PolicyPatchList,
	pub analyze: PolicyAnalyze,
}

impl FromStr for PolicyFile {
	type Err = crate::Error;

	fn from_str(s: &str) -> Result<Self> {
		let document =
			// Print miette::Report with Debug for full help text
			KdlDocument::from_str(s).map_err(|e| hc_error!("Policy file doesn't parse as valid KDL:\n{:?}", Report::from(e)))?;
		let nodes = document.nodes();

		let plugins: PolicyPluginList =
			extract_data(nodes).ok_or_else(|| hc_error!("Could not parse 'plugins'"))?;
		// `patch` is an optional section
		let patch: PolicyPatchList = extract_data(nodes).unwrap_or_default();
		let analyze: PolicyAnalyze =
			extract_data(nodes).ok_or_else(|| hc_error!("Could not parse 'analyze'"))?;

		Ok(Self {
			plugins,
			patch,
			analyze,
		})
	}
}

impl PolicyFile {
	/// retrieve the risk policy expression from a PolicyFile
	pub fn risk_policy(&self) -> Result<Expr> {
		let expr_str = self.analyze.investigate_policy.0.as_str();
		std_parse(expr_str)
			.map_err(|e| hc_error!("Malformed risk policy expression '{}': {}", expr_str, e))
	}

	/// Load policy from the given file.
	pub fn load_from(policy_path: &Path) -> Result<PolicyFile> {
		if policy_path.is_dir() {
			return Err(hc_error!(
				"Hipcheck policy path must be a file, not a directory."
			));
		}
		file::exists(policy_path)?;

		let raw_data = file::read_string(policy_path)?;
		let data = macros::preprocess_policy_file(raw_data.as_str(), policy_path)?;

		let policy = PolicyFile::from_str(&data)?;

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
	// @Todo - Revise this. Return `Result` not `Option`, take a `PluginPolicyName`,
	// instead of searching through `analyze` every time we should have a function
	// that returns plugin configs as a "view" of the combined analysis/patch
	// sections
	#[allow(unused)]
	pub fn get_config(&self, analysis_name: &str) -> Option<HashMap<String, Value>> {
		let opt_conf = self
			.analyze
			.find_analysis_by_name(analysis_name)
			.map(|analysis| analysis.config.map(|config| config.0).unwrap_or_default());
		// If plugin not listed in analyses, check `patch` section for config of dependencies
		if let Some(conf) = opt_conf {
			Some(conf)
		} else {
			let Ok(plugin_name) = PolicyPluginName::new(analysis_name) else {
				return None;
			};
			Some(
				self.patch
					.0
					.iter()
					.find(|x| x.name == plugin_name)
					.map(|p| p.config.0.clone())
					.unwrap_or_default(),
			)
		}
	}
}
