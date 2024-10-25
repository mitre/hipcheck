// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};

use pathbuf::pathbuf;

use crate::plugin::PluginId;

/// Plugins are stored with the following format `<path_to_plugin_cache>/<publisher>/<plugin_name>/<version>`
pub struct HcPluginCache {
	/// path to the root of the plugin cache
	path: PathBuf,
}

impl HcPluginCache {
	pub fn new(path: &Path) -> Self {
		let plugins_path = pathbuf![path, "plugins"];
		Self { path: plugins_path }
	}

	/// The folder in which a specific PluginID will be stored
	///
	/// `<path_to_plugin_cache>/<publisher>/<plugin_name>/<version>`
	pub fn plugin_download_dir(&self, plugin_id: &PluginId) -> PathBuf {
		self.path
			.join(plugin_id.publisher().as_ref())
			.join(plugin_id.name().as_ref())
			.join(plugin_id.version().as_ref())
	}

	/// The path to where the `plugin.kdl` file for a specific PluginId will be stored
	///
	/// `<path_to_plugin_cache>/<publisher>/<plugin_name>/<version>/plugin.kdl`
	pub fn plugin_kdl(&self, plugin_id: &PluginId) -> PathBuf {
		self.plugin_download_dir(plugin_id).join("plugin.kdl")
	}
}
