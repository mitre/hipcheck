// SPDX-License-Identifier: Apache-2.0

use std::fmt::Display;

use crate::plugin::{PluginName, PluginPublisher, PluginVersion, PluginVersionReq};

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
/// This structure is used to uniquely identify a plugin, when downloading/extracting
pub struct PluginId {
	publisher: PluginPublisher,
	name: PluginName,
	version: PluginVersion,
}

impl PluginId {
	pub fn new(publisher: PluginPublisher, name: PluginName, version: PluginVersion) -> Self {
		Self {
			publisher,
			name,
			version,
		}
	}

	pub fn publisher(&self) -> &PluginPublisher {
		&self.publisher
	}

	pub fn name(&self) -> &PluginName {
		&self.name
	}

	pub fn version(&self) -> &PluginVersion {
		&self.version
	}

	/// converts the `PluginId` to the format of the plugin identifier in a policy file
	///
	/// Example:
	/// `"mitre/git"`
	pub fn to_policy_file_plugin_identifier(&self) -> String {
		format!("{}/{}", self.publisher.0, self.name.0)
	}
}

impl Display for PluginId {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(
			f,
			"{} version {}",
			self.to_policy_file_plugin_identifier(),
			self.version.version
		)
	}
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
/// This structure is used to identify a plugin where a version range via Semver VersionReq is specified
pub struct PluginIdVersionRange {
	publisher: PluginPublisher,
	name: PluginName,
	version_req: PluginVersionReq,
}

impl PluginIdVersionRange {
	pub fn new(
		publisher: PluginPublisher,
		name: PluginName,
		version_req: PluginVersionReq,
	) -> Self {
		Self {
			publisher,
			name,
			version_req,
		}
	}

	pub fn publisher(&self) -> &PluginPublisher {
		&self.publisher
	}

	pub fn name(&self) -> &PluginName {
		&self.name
	}

	pub fn version(&self) -> &PluginVersionReq {
		&self.version_req
	}

	/// converts the `PluginIdVersionRange` to the format of the plugin identifier in a policy file
	///
	/// Example:
	/// `"mitre/git"`
	pub fn to_policy_file_plugin_identifier(&self) -> String {
		format!("{}/{}", self.publisher.0, self.name.0)
	}
}

impl Display for PluginIdVersionRange {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(
			f,
			"{} version {}",
			self.to_policy_file_plugin_identifier(),
			self.version_req.version_req
		)
	}
}
