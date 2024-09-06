// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};

use pathbuf::pathbuf;

use crate::plugin::{PluginName, PluginPublisher, PluginVersion};

/// Plugins are stored with the following format `<path_to_plugin_cache>/<publisher>/<plugin_name>/<version>`
pub struct HcPluginCache {
	path: PathBuf,
}

impl HcPluginCache {
	pub fn new(path: &Path) -> Self {
		let plugins_path = pathbuf![path, "plugins"];
		Self { path: plugins_path }
	}

	/// `<path_to_plugin_cache>/<publisher>/<plugin_name>/<version>`
	pub fn plugin_download_dir(
		&self,
		publisher: &PluginPublisher,
		name: &PluginName,
		version: &PluginVersion,
	) -> PathBuf {
		self.path.join(&publisher.0).join(&name.0).join(&version.0)
	}
}
