use super::{PluginName, PluginPublisher, PluginVersion};

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
/// This structure is used to uniquely identify a plugin, when downloading/extracting
pub struct PluginId {
	pub publisher: PluginPublisher,
	pub name: PluginName,
	pub version: PluginVersion,
}

impl PluginId {
	pub fn new(publisher: PluginPublisher, name: PluginName, version: PluginVersion) -> Self {
		Self {
			publisher,
			name,
			version,
		}
	}

	/// converts the `PluginId` to the format of the plugin identifier in a policy file
	///
	/// Example:
	/// `"mitre/git"`
	pub fn to_policy_file_plugin_identifier(&self) -> String {
		format!("{}/{}", self.publisher.0, self.name.0)
	}
}
