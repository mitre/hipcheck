// SPDX-License-Identifier: Apache-2.0

use crate::{
	error::Error,
	hc_error,
	plugin::{Arch, PluginIdVersionRange},
	policy::policy_file::ManifestLocation,
	util::fs::read_string,
};
use hipcheck_kdl::kdl::{KdlDocument, KdlNode};
use hipcheck_kdl::{extract_data, string_newtype_parse_kdl_node, ParseKdlNode, ToKdlNode};
use semver::{Version, VersionReq};
use std::{
	collections::HashMap,
	ops::Not,
	path::{Path, PathBuf},
	str::FromStr,
};

#[cfg(test)]
use crate::plugin::arch::KnownArch;

// NOTE: the implementation in this crate was largely derived from RFD #4

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PluginPublisher(pub String);
string_newtype_parse_kdl_node!(PluginPublisher, "publisher");

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PluginName(pub String);
string_newtype_parse_kdl_node!(PluginName, "name");

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PluginVersion {
	pub version: Version,
}

impl PluginVersion {
	pub fn new(value: &str) -> Result<Self, Error> {
		let version = Version::parse(value)?;
		Ok(Self { version })
	}
}
impl ParseKdlNode for PluginVersion {
	fn kdl_key() -> &'static str {
		"version"
	}

	fn parse_node(node: &KdlNode) -> Option<Self> {
		if node.name().to_string().as_str() != Self::kdl_key() {
			return None;
		}
		// Semver Version
		let version = Version::parse(node.entries().first()?.value().as_string()?).unwrap();
		Some(Self { version })
	}
}

impl AsRef<Version> for PluginVersion {
	fn as_ref(&self) -> &Version {
		&self.version
	}
}

impl ToKdlNode for PluginVersion {
	#[allow(unused)]
	fn to_kdl_node(&self) -> KdlNode {
		let mut node = KdlNode::new(Self::kdl_key());
		node.insert(0, self.version.to_string());
		node
	}
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PluginVersionReq {
	pub version_req: VersionReq,
}

impl PluginVersionReq {
	pub fn new(value: &str) -> Result<Self, Error> {
		// let version_req = VersionReq::parse(value)?;
		let version_req = validate_version_req(value)?;
		Ok(Self { version_req })
	}
}

fn validate_version_req(version_string: &str) -> Result<VersionReq, Error> {
	// If an exact version is provided, e.g., "0.4.2", a "=" will be prepended to it to
	// make the Semver VersionReq object treat the version as an exact version requirement
	let parsed_req = if Version::parse(version_string).is_ok() {
		VersionReq::parse(&format!("={}", version_string))
			.map_err(|e| hc_error!("Invalid version requirement: {}", e))?
	} else {
		VersionReq::parse(version_string)
			.map_err(|e| hc_error!("Invalid version requirement: {}", e))?
	};
	Ok(parsed_req)
}

impl ParseKdlNode for PluginVersionReq {
	fn kdl_key() -> &'static str {
		"version"
	}

	fn parse_node(node: &KdlNode) -> Option<Self> {
		if node.name().to_string().as_str() != Self::kdl_key() {
			return None;
		}
		// Semver Version Requirement
		let version_req = VersionReq::parse(node.entries().first()?.value().as_string()?).unwrap();
		Some(Self { version_req })
	}
}

impl AsRef<VersionReq> for PluginVersionReq {
	fn as_ref(&self) -> &VersionReq {
		&self.version_req
	}
}

impl ToKdlNode for PluginVersionReq {
	#[allow(unused)]
	fn to_kdl_node(&self) -> KdlNode {
		let mut node = KdlNode::new(Self::kdl_key());
		node.insert(0, self.version_req.to_string());
		node
	}
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct License(pub String);
string_newtype_parse_kdl_node!(License, "license");

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entrypoints(pub HashMap<Arch, String>);

impl Entrypoints {
	pub fn new() -> Self {
		Self(HashMap::new())
	}

	pub fn insert(&mut self, arch: Arch, entrypoint: String) -> Result<(), Error> {
		match self.0.insert(arch.clone(), entrypoint) {
			Some(_duplicate_key) => Err(hc_error!("Multiple entrypoints specified for {}", arch)),
			None => Ok(()),
		}
	}
}

impl ToKdlNode for Entrypoints {
	fn to_kdl_node(&self) -> KdlNode {
		let mut entrypoint_parent = KdlNode::new("entrypoint");
		let mut entrypoint_children = KdlDocument::new();
		let entrypoint_children_nodes = entrypoint_children.nodes_mut();
		for (arch, entrypoint) in self.0.iter() {
			let mut entry = KdlNode::new("on");
			entry.insert("arch", arch.to_string());
			entry.insert(0, entrypoint.to_owned());
			entrypoint_children_nodes.push(entry);
		}
		entrypoint_parent.set_children(entrypoint_children);
		entrypoint_parent
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
			let arch = Arch::from_str(entrypoint_spec.get("arch")?.as_string()?).ok()?;
			// per RFD #0004, the actual entrypoint is the first positional arg after "arch" and is
			// of type String
			let entrypoint = entrypoint_spec
				.entries()
				.get(1)?
				.value()
				.as_string()?
				.to_string();

			if let Err(_e) = entrypoints.insert(arch.clone(), entrypoint) {
				tracing::error!("Duplicate entrypoint detected for [{}]", arch);
				return None;
			}
		}
		Some(entrypoints)
	}
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginDependency {
	/// identifier for this PluginDependency
	pub plugin_id: PluginIdVersionRange,
	// NOTE: until Hipcheck supports a registry, this is effectively required
	pub manifest: Option<ManifestLocation>,
}

impl PluginDependency {
	#[cfg(test)]
	pub fn new(plugin_id: PluginIdVersionRange, manifest: Option<ManifestLocation>) -> Self {
		Self {
			plugin_id,
			manifest,
		}
	}
}

impl AsRef<PluginIdVersionRange> for PluginDependency {
	fn as_ref(&self) -> &PluginIdVersionRange {
		&self.plugin_id
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

		let version = match PluginVersionReq::new(node.get("version")?.as_string()?) {
			Ok(version) => version,
			Err(e) => {
				tracing::error!("{}", e);
				return None;
			}
		};

		let manifest = match node.get("manifest") {
			Some(manifest) => {
				let manifest_location = manifest.as_string()?;
				if let Ok(url) = url::Url::parse(manifest_location) {
					Some(ManifestLocation::Url(url))
				} else {
					Some(ManifestLocation::Local(PathBuf::from(manifest_location)))
				}
			}
			None => None,
		};
		let plugin_id = PluginIdVersionRange::new(publisher, name, version);

		Some(Self {
			plugin_id,
			manifest,
		})
	}
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct PluginDependencyList(pub Vec<PluginDependency>);

impl PluginDependencyList {
	pub fn new() -> Self {
		Self(Vec::new())
	}

	pub fn push(&mut self, dependency: PluginDependency) {
		self.0.push(dependency);
	}
}

impl ToKdlNode for PluginDependencyList {
	fn to_kdl_node(&self) -> KdlNode {
		let mut dependency_parent = KdlNode::new("dependencies");
		let mut dependency_children = KdlDocument::new();
		let dependency_children_nodes = dependency_children.nodes_mut();
		for dep in self.0.iter() {
			let mut entry = KdlNode::new("plugin");
			entry.insert(0, dep.plugin_id.to_policy_file_plugin_identifier());
			entry.insert("version", dep.plugin_id.version().version_req.to_string());
			if let Some(manifest) = &dep.manifest {
				entry.insert("manifest", manifest.to_string());
			}
			dependency_children_nodes.push(entry);
		}
		dependency_parent.set_children(dependency_children);
		dependency_parent
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
	pub fn get_entrypoint(&self, arch: &Arch) -> Option<String> {
		self.entrypoints.0.get(arch).cloned()
	}

	fn set_entrypoint(&mut self, arch: Arch, entrypoint: String) {
		self.entrypoints.0.insert(arch, entrypoint);
	}

	pub fn from_file<P>(path: P) -> Result<Self, Error>
	where
		P: AsRef<Path>,
	{
		Self::from_str(read_string(path)?.as_str())
	}

	pub fn get_entrypoint_for(&self, arch: &Arch) -> Result<String, Error> {
		self.entrypoints
			.0
			.get(arch)
			.map(String::from)
			.ok_or(hc_error!("No entrypoint for current arch ({})", arch))
	}

	/// Update the directory that an entrypoint is stored in
	///
	/// Returns previous entrypoint
	pub fn update_entrypoint<P>(&mut self, arch: &Arch, new_directory: P) -> Result<PathBuf, Error>
	where
		P: AsRef<Path>,
	{
		let curr_entrypoint_str = self
			.entrypoints
			.0
			.remove(arch)
			.ok_or(hc_error!("No entrypoint for current arch ({})", arch))?;

		let (opt_bin_path, args) = try_get_bin_for_entrypoint(&curr_entrypoint_str);
		let bin_path = opt_bin_path
			.map(PathBuf::from)
			.ok_or(hc_error!("Malformed entrypoint string"))?;

		fn new_path(new_directory: &Path, current_entrypoint: &Path) -> Result<PathBuf, Error> {
			let entrypoint_filename =
				new_directory.join(current_entrypoint.file_name().ok_or(hc_error!(
					"'{}' entrypoint does not contain a valid filename",
					current_entrypoint.to_string_lossy()
				))?);
			Ok(entrypoint_filename)
		}

		let new_bin_path = new_path(new_directory.as_ref(), &bin_path)?;
		let mut new_entrypoint = new_bin_path.to_string_lossy().to_string();
		if args.is_empty().not() {
			new_entrypoint.push(' ');
			new_entrypoint.push_str(&args.join(" "));
		}

		self.set_entrypoint(arch.clone(), new_entrypoint);
		Ok(bin_path)
	}

	/// convert a `PluginManifest` to a `KdlDocument`
	fn to_kdl(&self) -> KdlDocument {
		let mut document = KdlDocument::new();
		document.nodes_mut().extend([
			self.publisher.to_kdl_node(),
			self.name.to_kdl_node(),
			self.version.to_kdl_node(),
			self.license.to_kdl_node(),
			self.entrypoints.to_kdl_node(),
			self.dependencies.to_kdl_node(),
		]);
		document
	}

	/// convert `PluginManifest` to a KDL-formatted String
	pub fn to_kdl_formatted_string(&self) -> String {
		self.to_kdl().to_string()
	}
}

impl FromStr for PluginManifest {
	type Err = crate::Error;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		let document = KdlDocument::from_str(s)
			.map_err(|e| hc_error!("Error parsing plugin manifest file: {}", e))?;
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
		// Not a required field
		let dependencies: PluginDependencyList = extract_data(nodes).unwrap_or_default();

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

pub fn try_get_bin_for_entrypoint(entrypoint: &str) -> (Option<&str>, Vec<&str>) {
	let mut split = entrypoint.split_whitespace();
	(split.next(), split.collect())
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
			PluginVersion::new("0.1.0").unwrap(),
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
				Arch::Known(KnownArch::Aarch64AppleDarwin),
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
		expected
			.insert(
				Arch::Known(KnownArch::Aarch64AppleDarwin),
				"./hc-mitre-affiliation".to_owned(),
			)
			.unwrap();
		expected
			.insert(
				Arch::Known(KnownArch::X86_64AppleDarwin),
				"./hc-mitre-affiliation".to_owned(),
			)
			.unwrap();
		expected
			.insert(
				Arch::Known(KnownArch::X86_64UnknownLinuxGnu),
				"./hc-mitre-affiliation".to_owned(),
			)
			.unwrap();
		expected
			.insert(
				Arch::Known(KnownArch::X86_64PcWindowsMsvc),
				"./hc-mitre-affiliation".to_owned(),
			)
			.unwrap();
		assert_eq!(Entrypoints::parse_node(&node).unwrap(), expected)
	}

	#[test]
	fn test_parsing_plugin_dependency() {
		let dep = r#"plugin "mitre/git" version="^0.1" manifest="https://github.com/mitre/hipcheck/blob/main/plugin/dist/mitre-git.kdl""#;
		let node = KdlNode::from_str(dep).unwrap();
		assert_eq!(
			PluginDependency::parse_node(&node).unwrap(),
			PluginDependency::new(
				PluginIdVersionRange::new(
					PluginPublisher("mitre".to_string()),
					PluginName("git".to_string()),
					PluginVersionReq::new("^0.1").unwrap(),
				),
				Some(ManifestLocation::Url(
					Url::parse(
						"https://github.com/mitre/hipcheck/blob/main/plugin/dist/mitre-git.kdl"
					)
					.unwrap()
				))
			)
		);
	}

	#[test]
	fn test_parsing_plugin_dependency_list() {
		let dependencies = r#"dependencies {
  plugin "mitre/git" version="^0.1" manifest="https://github.com/mitre/hipcheck/blob/main/plugin/dist/mitre-git.kdl"
  plugin "mitre/plugin2" version="^0.4" manifest="https://github.com/mitre/hipcheck/blob/main/plugin/dist/mitre-plugin2.kdl"
}"#;
		let node = KdlNode::from_str(dependencies).unwrap();
		let mut expected = PluginDependencyList::new();
		expected.push(PluginDependency::new(
			PluginIdVersionRange::new(
				PluginPublisher("mitre".to_string()),
				PluginName("git".to_string()),
				PluginVersionReq::new("^0.1").unwrap(),
			),
			Some(
				ManifestLocation::Url(
					url::Url::parse(
						"https://github.com/mitre/hipcheck/blob/main/plugin/dist/mitre-git.kdl",
					)
					.unwrap(),
				)
				.to_owned(),
			),
		));
		expected.push(PluginDependency::new(
			PluginIdVersionRange::new(
				PluginPublisher("mitre".to_string()),
				PluginName("plugin2".to_string()),
				PluginVersionReq::new("^0.4").unwrap(),
			),
			Some(ManifestLocation::Url(
				url::Url::parse(
					"https://github.com/mitre/hipcheck/blob/main/plugin/dist/mitre-plugin2.kdl",
				)
				.unwrap(),
			)),
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
  plugin "mitre/git" version="^0.1" manifest="https://github.com/mitre/hipcheck/blob/main/plugin/dist/mitre-git.kdl"
}"#;
		let plugin_manifest = PluginManifest::from_str(file_contents).unwrap();

		let mut entrypoints = Entrypoints::new();
		entrypoints
			.insert(
				Arch::Known(KnownArch::Aarch64AppleDarwin),
				"./hc-mitre-affiliation".to_owned(),
			)
			.unwrap();
		entrypoints
			.insert(
				Arch::Known(KnownArch::X86_64AppleDarwin),
				"./hc-mitre-affiliation".to_owned(),
			)
			.unwrap();
		entrypoints
			.insert(
				Arch::Known(KnownArch::X86_64UnknownLinuxGnu),
				"./hc-mitre-affiliation".to_owned(),
			)
			.unwrap();
		entrypoints
			.insert(
				Arch::Known(KnownArch::X86_64PcWindowsMsvc),
				"./hc-mitre-affiliation".to_owned(),
			)
			.unwrap();

		let mut dependencies = PluginDependencyList::new();
		dependencies.push(PluginDependency::new(
			PluginIdVersionRange::new(
				PluginPublisher("mitre".to_string()),
				PluginName("git".to_string()),
				PluginVersionReq::new("^0.1").unwrap(),
			),
			Some(ManifestLocation::Url(
				url::Url::parse(
					"https://github.com/mitre/hipcheck/blob/main/plugin/dist/mitre-git.kdl",
				)
				.unwrap(),
			)),
		));

		let expected_manifest = PluginManifest {
			publisher: PluginPublisher::new("mitre".to_owned()),
			name: PluginName::new("affiliation".to_owned()),
			version: PluginVersion::new("0.1.0").unwrap(),
			license: License::new("Apache-2.0".to_owned()),
			entrypoints,
			dependencies,
		};
		assert_eq!(plugin_manifest, expected_manifest);
	}

	#[test]
	fn test_to_kdl() {
		let mut entrypoints = Entrypoints::new();
		entrypoints
			.insert(
				Arch::Known(KnownArch::Aarch64AppleDarwin),
				"./target/debug/activity".to_owned(),
			)
			.unwrap();
		entrypoints
			.insert(
				Arch::Known(KnownArch::X86_64AppleDarwin),
				"./target/debug/activity".to_owned(),
			)
			.unwrap();
		entrypoints
			.insert(
				Arch::Known(KnownArch::X86_64UnknownLinuxGnu),
				"./target/debug/activity".to_owned(),
			)
			.unwrap();
		entrypoints
			.insert(
				Arch::Known(KnownArch::X86_64PcWindowsMsvc),
				"./target/debug/activity".to_owned(),
			)
			.unwrap();

		let mut dependencies = PluginDependencyList::new();
		dependencies.push(PluginDependency::new(
			PluginIdVersionRange::new(
				PluginPublisher::new("mitre".to_owned()),
				PluginName::new("git".to_owned()),
				PluginVersionReq::new("^0.1").unwrap(),
			),
			Some(ManifestLocation::Local("./plugins/git/plugin.kdl".into())),
		));

		let plugin_manifest = PluginManifest {
			publisher: PluginPublisher::new("mitre".to_owned()),
			name: PluginName::new("activity".to_owned()),
			version: PluginVersion::new("0.1.0").unwrap(),
			license: License::new("Apache-2.0".to_owned()),
			entrypoints,
			dependencies,
		};

		let plugin_manifest_string = plugin_manifest.to_kdl_formatted_string();

		assert_eq!(
			plugin_manifest,
			PluginManifest::from_str(&plugin_manifest_string).unwrap()
		)
	}

	#[test]
	fn validate_accepted_version_reqs() {
		let version_req = "1.0.0";
		let parsed_version_req = validate_version_req(version_req).unwrap();
		assert_eq!(parsed_version_req, VersionReq::parse("=1.0.0").unwrap());
		let version_req = "^1.0.0";
		let parsed_version_req = validate_version_req(version_req).unwrap();
		assert_eq!(parsed_version_req, VersionReq::parse("^1.0.0").unwrap());
		let version_req = ">=1.2.5, <1.5.0";
		let parsed_version_req = validate_version_req(version_req).unwrap();
		assert_eq!(
			parsed_version_req,
			VersionReq::parse(">=1.2.5, <1.5.0").unwrap()
		);
	}
}
