// SPDX-License-Identifier: Apache-2.0

use crate::{
	error::Error,
	hc_error,
	plugin::supported_arch::SupportedArch,
	string_newtype_parse_kdl_node,
	util::kdl::{extract_data, ParseKdlNode},
};
use core::panic;
use kdl::{KdlDocument, KdlEntry, KdlNode, KdlValue};
use petgraph::graphmap::NeighborsDirected;
use std::{
	collections::HashMap,
	fmt::{write, Display},
	str::FromStr,
};

// NOTE: the implementation in this crate was largely derived from RFD #4

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PluginPublisher(pub String);
string_newtype_parse_kdl_node!(PluginPublisher, "publisher");

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PluginName(pub String);
string_newtype_parse_kdl_node!(PluginName, "name");

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PluginVersion(pub String);
string_newtype_parse_kdl_node!(PluginVersion, "version");

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct License(pub String);
string_newtype_parse_kdl_node!(License, "license");

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entrypoints(pub HashMap<SupportedArch, String>);

impl Entrypoints {
	pub fn new() -> Self {
		Self(HashMap::new())
	}

	pub fn insert(&mut self, arch: SupportedArch, entrypoint: String) -> Result<(), Error> {
		match self.0.insert(arch, entrypoint) {
			Some(_duplicate_key) => Err(hc_error!("Multiple entrypoints specified for {}", arch)),
			None => Ok(()),
		}
	}

	pub fn iter(&self) -> impl Iterator<Item = (&SupportedArch, &String)> {
		self.0.iter()
	}
}

impl ParseKdlNode for Entrypoints {
	fn kdl_key() -> &'static str {
		"entrypoint"
	}

	fn parse_node(node: &KdlNode) -> Option<Self> {
		if node.name().to_string().as_str() != Self::kdl_key() {
			return None;
		}
		let mut entrypoints = Entrypoints::new();
		for entrypoint_spec in node.children()?.nodes() {
			// per RFD #0004, the value for "arch" is of type String
			let arch =
				SupportedArch::from_str(entrypoint_spec.get("arch")?.value().as_string()?).ok()?;
			// per RFD #0004, the actual entrypoint is the first positional arg after "arch" and is
			// of type String
			let entrypoint = entrypoint_spec
				.entries()
				.get(1)?
				.value()
				.as_string()?
				.to_string();
			if let Err(e) = entrypoints.insert(arch, entrypoint) {
				log::error!("Duplicate entrypoint detected for [{}]", arch);
				return None;
			}
		}
		Some(entrypoints)
	}
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginDependency {
	pub publisher: PluginPublisher,
	pub name: PluginName,
	pub version: PluginVersion,
	// NOTE: until Hipcheck supports a registry, this is effectively required
	pub manifest: Option<url::Url>,
}

impl PluginDependency {
	pub fn new(
		publisher: PluginPublisher,
		name: PluginName,
		version: PluginVersion,
		manifest: Option<url::Url>,
	) -> Self {
		Self {
			publisher,
			name,
			version,
			manifest,
		}
	}
}

impl ParseKdlNode for PluginDependency {
	fn kdl_key() -> &'static str {
		"plugin"
	}

	fn parse_node(node: &KdlNode) -> Option<Self> {
		if node.name().to_string().as_str() != Self::kdl_key() {
			return None;
		}

		// per RFD #4, the name is the first positional entry and has type String and is of the format `<publisher>/<name>`
		let publisher_and_name = node.entries().first()?.value().as_string()?;
		let (publisher, name) = match publisher_and_name.split_once('/') {
			Some((publisher, name)) => (
				PluginPublisher(publisher.to_string()),
				PluginName(name.to_string()),
			),
			None => return None,
		};

		let version = PluginVersion(node.get("version")?.value().as_string()?.to_string());
		let manifest = match node.get("manifest") {
			Some(manifest) => {
				let raw_url = manifest.value().as_string()?;
				Some(url::Url::parse(raw_url).ok()?)
			}
			None => None,
		};

		Some(Self {
			name,
			publisher,
			version,
			manifest,
		})
	}
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginDependencyList(pub Vec<PluginDependency>);

impl PluginDependencyList {
	pub fn new() -> Self {
		Self(Vec::new())
	}

	pub fn with_capacity(capacity: usize) -> Self {
		Self(Vec::with_capacity(capacity))
	}

	pub fn push(&mut self, dependency: PluginDependency) {
		self.0.push(dependency);
	}

	pub fn pop(&mut self) -> Option<PluginDependency> {
		self.0.pop()
	}
}

impl ParseKdlNode for PluginDependencyList {
	fn kdl_key() -> &'static str {
		"dependencies"
	}

	fn parse_node(node: &KdlNode) -> Option<Self> {
		if node.name().to_string().as_str() != Self::kdl_key() {
			return None;
		}

		let mut dependencies = Self::new();

		for node in node.children()?.nodes() {
			if let Some(dep) = PluginDependency::parse_node(node) {
				dependencies.push(dep);
			}
		}

		Some(dependencies)
	}
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginManifest {
	pub publisher: PluginPublisher,
	pub name: PluginName,
	pub version: PluginVersion,
	pub license: License,
	pub entrypoints: Entrypoints,
	pub dependencies: PluginDependencyList,
}

impl PluginManifest {
	pub fn get_entrypoint(&self, arch: SupportedArch) -> Option<String> {
		self.entrypoints.0.get(&arch).cloned()
	}
}

impl FromStr for PluginManifest {
	type Err = crate::Error;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		let document = KdlDocument::from_str(s)
			.map_err(|e| hc_error!("Error parsing plugin manifest file: {e}"))?;
		let nodes = document.nodes();

		let publisher: PluginPublisher =
			extract_data(nodes).ok_or_else(|| hc_error!("Could not parse 'publisher'"))?;
		let name: PluginName =
			extract_data(nodes).ok_or_else(|| hc_error!("Could not parse 'name'"))?;
		let version: PluginVersion =
			extract_data(nodes).ok_or_else(|| hc_error!("Could not parse 'version'"))?;
		let license: License =
			extract_data(nodes).ok_or_else(|| hc_error!("Could not parse 'license'"))?;
		let entrypoints: Entrypoints =
			extract_data(nodes).ok_or_else(|| hc_error!("Could not parse 'entrypoint'"))?;
		let dependencies: PluginDependencyList =
			extract_data(nodes).ok_or_else(|| hc_error!("Could not parse 'dependencies'"))?;

		Ok(Self {
			publisher,
			name,
			version,
			license,
			entrypoints,
			dependencies,
		})
	}
}

#[cfg(test)]
mod test {

	use url::Url;

	use super::*;

	#[test]
	fn test_parsing_publisher() {
		let data = r#"publisher "mitre""#;
		let node = KdlNode::from_str(data).unwrap();
		assert_eq!(
			PluginPublisher::new("mitre".to_owned()),
			PluginPublisher::parse_node(&node).unwrap()
		)
	}

	#[test]
	fn test_parsing_version() {
		let data = r#"version "0.1.0""#;
		let node = KdlNode::from_str(data).unwrap();
		assert_eq!(
			PluginVersion::new("0.1.0".to_owned()),
			PluginVersion::parse_node(&node).unwrap()
		)
	}

	#[test]
	fn test_parsing_name() {
		let data = r#"name "affiliation""#;
		let node = KdlNode::from_str(data).unwrap();
		assert_eq!(
			PluginName::new("affiliation".to_owned()),
			PluginName::parse_node(&node).unwrap()
		);
	}

	#[test]
	fn test_parsing_license() {
		let data = r#"license "Apache-2.0""#;
		let node = KdlNode::from_str(data).unwrap();
		assert_eq!(
			License::new("Apache-2.0".to_owned()),
			License::parse_node(&node).unwrap()
		);
	}

	#[test]
	fn test_parsing_entrypoint() {
		let single_entrypoint = r#"entrypoint {
    on arch="aarch64-apple-darwin" "./hc-mitre-affiliation"
    }"#;
		let node = KdlNode::from_str(single_entrypoint).unwrap();
		let mut expected = Entrypoints::new();
		expected
			.insert(
				SupportedArch::Aarch64AppleDarwin,
				"./hc-mitre-affiliation".to_owned(),
			)
			.unwrap();
		assert_eq!(expected, Entrypoints::parse_node(&node).unwrap());
	}

	/// Currently, None is returned if there are two entrypoints for one architecture!
	#[test]
	fn test_parsing_duplicate_arch_entrypoint() {
		let duplicate_arch = r#"entrypoint {
    on arch="aarch64-apple-darwin" "./hc-mitre-affiliation"
    on arch="aarch64-apple-darwin" "./hc-mitre-affiliation"
    }"#;
		let node = KdlNode::from_str(duplicate_arch).unwrap();
		assert!(Entrypoints::parse_node(&node).is_none())
	}

	#[test]
	fn test_parsing_multiple_entrypoint() {
		let multiple_entrypoint = r#"entrypoint {
  on arch="aarch64-apple-darwin" "./hc-mitre-affiliation"
  on arch="x86_64-apple-darwin" "./hc-mitre-affiliation"
  on arch="x86_64-unknown-linux-gnu" "./hc-mitre-affiliation"
  on arch="x86_64-pc-windows-msvc" "./hc-mitre-affiliation"
}"#;
		let node = KdlNode::from_str(multiple_entrypoint).unwrap();
		let mut expected = Entrypoints::new();
		expected.insert(
			SupportedArch::Aarch64AppleDarwin,
			"./hc-mitre-affiliation".to_owned(),
		);
		expected.insert(
			SupportedArch::X86_64AppleDarwin,
			"./hc-mitre-affiliation".to_owned(),
		);
		expected.insert(
			SupportedArch::X86_64UnknownLinuxGnu,
			"./hc-mitre-affiliation".to_owned(),
		);
		expected.insert(
			SupportedArch::X86_64PcWindowsMsvc,
			"./hc-mitre-affiliation".to_owned(),
		);
		assert_eq!(Entrypoints::parse_node(&node).unwrap(), expected)
	}

	#[test]
	fn test_parsing_plugin_dependency() {
		let dep = r#"plugin "mitre/git" version="0.1.0" manifest="https://github.com/mitre/hipcheck/blob/main/plugin/dist/mitre-git.kdl""#;
		let node = KdlNode::from_str(dep).unwrap();
		assert_eq!(
			PluginDependency::parse_node(&node).unwrap(),
			PluginDependency::new(
				PluginPublisher("mitre".to_string()),
				PluginName("git".to_string()),
				PluginVersion("0.1.0".to_string()),
				Some(
					Url::parse(
						"https://github.com/mitre/hipcheck/blob/main/plugin/dist/mitre-git.kdl"
					)
					.unwrap()
				)
			)
		);
	}

	#[test]
	fn test_parsing_plugin_dependency_list() {
		let dependencies = r#"dependencies {
  plugin "mitre/git" version="0.1.0" manifest="https://github.com/mitre/hipcheck/blob/main/plugin/dist/mitre-git.kdl"
  plugin "mitre/plugin2" version="0.4.0" manifest="https://github.com/mitre/hipcheck/blob/main/plugin/dist/mitre-plugin2.kdl"
}"#;
		let node = KdlNode::from_str(dependencies).unwrap();
		let mut expected = PluginDependencyList::new();
		expected.push(PluginDependency::new(
			PluginPublisher("mitre".to_string()),
			PluginName("git".to_string()),
			PluginVersion("0.1.0".to_string()),
			Some(
				url::Url::parse(
					"https://github.com/mitre/hipcheck/blob/main/plugin/dist/mitre-git.kdl",
				)
				.unwrap(),
			)
			.to_owned(),
		));
		expected.push(PluginDependency::new(
			PluginPublisher("mitre".to_string()),
			PluginName("plugin2".to_string()),
			PluginVersion("0.4.0".to_string()),
			Some(
				url::Url::parse(
					"https://github.com/mitre/hipcheck/blob/main/plugin/dist/mitre-plugin2.kdl",
				)
				.unwrap(),
			),
		));
		assert_eq!(PluginDependencyList::parse_node(&node).unwrap(), expected);
	}

	#[test]
	fn test_parsing_entire_plugin_manifest_file() {
		let file_contents = r#"publisher "mitre"
name "affiliation"
version "0.1.0"
license "Apache-2.0"
entrypoint {
  on arch="aarch64-apple-darwin" "./hc-mitre-affiliation"
  on arch="x86_64-apple-darwin" "./hc-mitre-affiliation"
  on arch="x86_64-unknown-linux-gnu" "./hc-mitre-affiliation"
  on arch="x86_64-pc-windows-msvc" "./hc-mitre-affiliation"
}

dependencies {
  plugin "mitre/git" version="0.1.0" manifest="https://github.com/mitre/hipcheck/blob/main/plugin/dist/mitre-git.kdl"
}"#;
		let plugin_manifest = PluginManifest::from_str(file_contents).unwrap();

		let mut entrypoints = Entrypoints::new();
		entrypoints.insert(
			SupportedArch::Aarch64AppleDarwin,
			"./hc-mitre-affiliation".to_owned(),
		);
		entrypoints.insert(
			SupportedArch::X86_64AppleDarwin,
			"./hc-mitre-affiliation".to_owned(),
		);
		entrypoints.insert(
			SupportedArch::X86_64UnknownLinuxGnu,
			"./hc-mitre-affiliation".to_owned(),
		);
		entrypoints.insert(
			SupportedArch::X86_64PcWindowsMsvc,
			"./hc-mitre-affiliation".to_owned(),
		);

		let mut dependencies = PluginDependencyList::new();
		dependencies.push(PluginDependency::new(
			PluginPublisher("mitre".to_string()),
			PluginName("git".to_string()),
			PluginVersion("0.1.0".to_string()),
			Some(
				url::Url::parse(
					"https://github.com/mitre/hipcheck/blob/main/plugin/dist/mitre-git.kdl",
				)
				.unwrap(),
			),
		));

		let expected_manifest = PluginManifest {
			publisher: PluginPublisher::new("mitre".to_owned()),
			name: PluginName::new("affiliation".to_owned()),
			version: PluginVersion::new("0.1.0".to_owned()),
			license: License::new("Apache-2.0".to_owned()),
			entrypoints,
			dependencies,
		};
		assert_eq!(plugin_manifest, expected_manifest);
	}
}
