// SPDX-License-Identifier: Apache-2.0

//! Data types and functions for use in parsing policy KDL files

use crate::{
	error::Result,
	hc_error,
	kdl_helper::{extract_data, ParseKdlNode},
	plugin::{PluginName, PluginPublisher, PluginVersion},
	string_newtype_parse_kdl_node,
};

use kdl::KdlNode;
use std::{collections::HashMap, fmt, fmt::Display};
use url::Url;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolicyPlugin {
	pub name: PolicyPluginName,
	pub version: PluginVersion,
	pub manifest: Option<Url>,
}

impl PolicyPlugin {
	#[allow(dead_code)]
	pub fn new(name: PolicyPluginName, version: PluginVersion, manifest: Option<Url>) -> Self {
		Self {
			name,
			version,
			manifest,
		}
	}
}

impl ParseKdlNode for PolicyPlugin {
	fn kdl_key() -> &'static str {
		"plugin"
	}

	fn parse_node(node: &KdlNode) -> Option<Self> {
		if node.name().to_string().as_str() != Self::kdl_key() {
			return None;
		}

		// per RFD #0004, the name is the first positional entry and has type String
		// We split it into separate publisher and name fields here for use when downloading plugins downstream
		let full_name = node.entries().first()?.value().as_string()?;
		let name = match PolicyPluginName::new(full_name) {
			Ok(name) => name,
			Err(e) => {
				log::error!("{}", e);
				return None;
			}
		};
		let version = PluginVersion::new(node.get("version")?.value().as_string()?.to_string());

		// The manifest is technically optional, as there should be a default Hipcheck plugin artifactory sometime in the future
		// But for now it is essentially mandatory, so a plugin without a manifest will return an error downstream
		let manifest = match node.get("manifest") {
			Some(entry) => {
				let raw_url = entry.value().as_string()?;
				match Url::parse(raw_url) {
					Ok(url) => Some(url),
					Err(_) => {
						log::error!("Unable to parse provided manifest URL {} for plugin {} in the policy file", raw_url, name.to_string());
						return None;
					}
				}
			}
			None => None,
		};

		Some(Self {
			name,
			version,
			manifest,
		})
	}
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolicyPluginList(pub Vec<PolicyPlugin>);

impl PolicyPluginList {
	pub fn new() -> Self {
		Self(Vec::new())
	}

	#[allow(dead_code)]
	pub fn with_capacity(capacity: usize) -> Self {
		Self(Vec::with_capacity(capacity))
	}

	pub fn push(&mut self, plugin: PolicyPlugin) {
		self.0.push(plugin);
	}

	#[allow(dead_code)]
	pub fn pop(&mut self) -> Option<PolicyPlugin> {
		self.0.pop()
	}
}

impl ParseKdlNode for PolicyPluginList {
	fn kdl_key() -> &'static str {
		"plugins"
	}

	fn parse_node(node: &KdlNode) -> Option<Self> {
		if node.name().to_string().as_str() != Self::kdl_key() {
			return None;
		}

		let mut plugins = Self::new();

		for node in node.children()?.nodes() {
			if let Some(dep) = PolicyPlugin::parse_node(node) {
				plugins.push(dep);
			}
		}

		Some(plugins)
	}
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolicyConfig(pub HashMap<String, String>);

impl PolicyConfig {
	pub fn new() -> Self {
		Self(HashMap::new())
	}

	pub fn insert(&mut self, description: String, info: String) -> Result<()> {
		match self.0.insert(description.clone(), info) {
			Some(_duplicate_key) => Err(hc_error!(
				"Duplicate configuration information specified for {}",
				description
			)),
			None => Ok(()),
		}
	}

	#[allow(dead_code)]
	pub fn get(self, description: &str) -> Option<String> {
		self.0.get(description).map(|info| info.to_string())
	}

	#[allow(dead_code)]
	pub fn iter(&self) -> impl Iterator<Item = (&String, &String)> {
		self.0.iter()
	}
}

impl ParseKdlNode for PolicyConfig {
	fn kdl_key() -> &'static str {
		"config"
	}

	fn parse_node(node: &KdlNode) -> Option<Self> {
		let mut config = PolicyConfig::new();
		for node in node.children()?.nodes() {
			let description = node.name().to_string();
			if let Some(info) = node.entries().first() {
				if config
					.insert(description.clone(), info.value().as_string()?.to_string())
					.is_err()
				{
					log::error!(
						"Duplicate configuration information detected for {}",
						description
					);
					return None;
				}
			}
		}
		Some(config)
	}
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolicyAnalysis {
	pub name: PolicyPluginName,
	pub policy_expression: Option<String>,
	pub weight: Option<u16>,
	pub config: Option<PolicyConfig>,
}

impl PolicyAnalysis {
	#[allow(dead_code)]
	pub fn new(
		name: PolicyPluginName,
		policy_expression: Option<String>,
		weight: Option<u16>,
		config: Option<PolicyConfig>,
	) -> Self {
		Self {
			name,
			policy_expression,
			weight,
			config,
		}
	}
}

impl ParseKdlNode for PolicyAnalysis {
	fn kdl_key() -> &'static str {
		"analysis"
	}

	fn parse_node(node: &KdlNode) -> Option<Self> {
		if node.name().to_string().as_str() != Self::kdl_key() {
			return None;
		}

		let full_name = node.entries().first()?.value().as_string()?;
		let name = match PolicyPluginName::new(full_name) {
			Ok(name) => name,
			Err(e) => {
				log::error!("{}", e);
				return None;
			}
		};
		let policy_expression = match node.get("policy") {
			Some(entry) => Some(entry.value().as_string()?.to_string()),
			None => None,
		};
		let weight = match node.get("weight") {
			Some(entry) => Some(entry.value().as_i64()? as u16),
			None => None,
		};

		let config = match node.children() {
			Some(_) => PolicyConfig::parse_node(node),
			None => None,
		};

		Some(Self {
			name,
			policy_expression,
			weight,
			config,
		})
	}
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolicyCategory {
	pub name: String,
	pub weight: Option<u16>,
	pub children: Vec<PolicyCategoryChild>,
}

impl PolicyCategory {
	#[allow(dead_code)]
	pub fn new(name: String, weight: Option<u16>) -> Self {
		Self {
			name,
			weight,
			children: Vec::new(),
		}
	}

	#[allow(dead_code)]
	pub fn with_capacity(name: String, weight: Option<u16>, capacity: usize) -> Self {
		Self {
			name,
			weight,
			children: Vec::with_capacity(capacity),
		}
	}

	#[allow(dead_code)]
	pub fn push(&mut self, child: PolicyCategoryChild) {
		self.children.push(child);
	}

	#[allow(dead_code)]
	pub fn pop(&mut self) -> Option<PolicyCategoryChild> {
		self.children.pop()
	}

	pub fn find_analysis_by_name(&self, name: &str) -> Option<PolicyAnalysis> {
		self.children
			.iter()
			.find_map(|child| child.find_analysis_by_name(name))
	}
}

impl ParseKdlNode for PolicyCategory {
	fn kdl_key() -> &'static str {
		"category"
	}

	fn parse_node(node: &KdlNode) -> Option<Self> {
		if node.name().to_string().as_str() != Self::kdl_key() {
			return None;
		}

		let name = node.entries().first()?.value().as_string()?.to_string();
		let weight = match node.get("weight") {
			Some(entry) => Some(entry.value().as_i64()? as u16),
			None => None,
		};

		let mut children = Vec::new();

		// A category can contain both analyses and further subcategories
		for node in node.children()?.nodes() {
			if node.name().to_string().as_str() == "analysis" {
				if let Some(analysis) = PolicyAnalysis::parse_node(node) {
					children.push(PolicyCategoryChild::Analysis(analysis));
				}
			} else if node.name().to_string().as_str() == "category" {
				if let Some(category) = PolicyCategory::parse_node(node) {
					children.push(PolicyCategoryChild::Category(category));
				}
			}
		}

		Some(Self {
			name,
			weight,
			children,
		})
	}
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PolicyCategoryChild {
	Analysis(PolicyAnalysis),
	Category(PolicyCategory),
}

impl PolicyCategoryChild {
	fn find_analysis_by_name(&self, name: &str) -> Option<PolicyAnalysis> {
		match self {
			PolicyCategoryChild::Analysis(analysis) => {
				let analysis_name = format!(
					"{}/{}",
					analysis.name.publisher.0.as_str(),
					analysis.name.name.0.as_str()
				);
				if analysis_name.as_str() == name {
					Some(analysis.clone())
				} else {
					None
				}
			}
			PolicyCategoryChild::Category(category) => category.find_analysis_by_name(name),
		}
	}
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvestigatePolicy(pub String);
string_newtype_parse_kdl_node!(InvestigatePolicy, "investigate");

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvestigateIfFail(Vec<PolicyPluginName>);

impl InvestigateIfFail {
	#[allow(dead_code)]
	pub fn new() -> Self {
		Self(Vec::new())
	}

	#[allow(dead_code)]
	pub fn with_capacity(capacity: usize) -> Self {
		Self(Vec::with_capacity(capacity))
	}

	#[allow(dead_code)]
	pub fn push(&mut self, plugin_name: &str) {
		if let Ok(plugin) = PolicyPluginName::new(plugin_name) {
			self.0.push(plugin);
		}
	}

	#[allow(dead_code)]
	pub fn push_plugin(&mut self, plugin: PolicyPluginName) {
		self.0.push(plugin);
	}

	#[allow(dead_code)]
	pub fn pop(&mut self) -> Option<PolicyPluginName> {
		self.0.pop()
	}
}

impl ParseKdlNode for InvestigateIfFail {
	fn kdl_key() -> &'static str {
		"investigate-if-fail"
	}

	fn parse_node(node: &KdlNode) -> Option<Self> {
		if node.name().to_string().as_str() != Self::kdl_key() {
			return None;
		}

		let mut policies = Vec::new();

		for node in node.entries() {
			// Trim leading and trailing quotation marks from each policy in the list
			let mut policy = node.value().to_string();
			policy.remove(0);
			policy.pop();
			policies.push(PolicyPluginName::new(&policy).ok()?)
		}

		Some(Self(policies))
	}
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolicyAnalyze {
	pub investigate_policy: InvestigatePolicy,
	pub if_fail: Option<InvestigateIfFail>,
	pub categories: Vec<PolicyCategory>,
}

impl PolicyAnalyze {
	#[allow(dead_code)]
	pub fn new(investigate_policy: InvestigatePolicy, if_fail: Option<InvestigateIfFail>) -> Self {
		Self {
			investigate_policy,
			if_fail,
			categories: Vec::new(),
		}
	}

	#[allow(dead_code)]
	pub fn with_capacity(
		investigate_policy: InvestigatePolicy,
		if_fail: Option<InvestigateIfFail>,
		capacity: usize,
	) -> Self {
		Self {
			investigate_policy,
			if_fail,
			categories: Vec::with_capacity(capacity),
		}
	}

	#[allow(dead_code)]
	pub fn push(&mut self, category: PolicyCategory) {
		self.categories.push(category);
	}

	#[allow(dead_code)]
	pub fn pop(&mut self) -> Option<PolicyCategory> {
		self.categories.pop()
	}

	pub fn find_analysis_by_name(&self, name: &str) -> Option<PolicyAnalysis> {
		self.categories
			.iter()
			.find_map(|category| category.find_analysis_by_name(name))
	}
}

impl ParseKdlNode for PolicyAnalyze {
	fn kdl_key() -> &'static str {
		"analyze"
	}

	fn parse_node(node: &KdlNode) -> Option<Self> {
		if node.name().to_string().as_str() != Self::kdl_key() {
			return None;
		}

		let nodes = node.children()?.nodes();

		let investigate_policy: InvestigatePolicy = extract_data(nodes)?;
		let if_fail: Option<InvestigateIfFail> = extract_data(nodes);

		let mut categories = Vec::new();

		for node in nodes {
			if node.name().to_string().as_str() == "category" {
				if let Some(category) = PolicyCategory::parse_node(node) {
					categories.push(category);
				}
			}
		}

		Some(Self {
			investigate_policy,
			if_fail,
			categories,
		})
	}
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PolicyPluginName {
	pub publisher: PluginPublisher,
	pub name: PluginName,
}

impl PolicyPluginName {
	pub fn new(full_name: &str) -> Result<Self> {
		let parsed_name: Vec<&str> = full_name.split('/').collect();
		if parsed_name.len() > 1 {
			let publisher = PluginPublisher::new(parsed_name[0].to_string());
			let name = PluginName::new(parsed_name[1].to_string());
			Ok(Self { publisher, name })
		} else {
			Err(hc_error!(
				"Provided policy {} is not in the format {{publisher}}/{{name}}",
				full_name
			))
		}
	}
}

impl Display for PolicyPluginName {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{}/{}", self.publisher.0, self.name.0)
	}
}
