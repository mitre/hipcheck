// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};

use pathbuf::pathbuf;

use crate::plugin::{PluginName, PluginPublisher, PluginVersion, SupportedArch};

/// Plugins are stored with the following format `<path_to_plugin_cache>/<publisher>/<plugin_name>/<version>/<arch>`
pub struct HcPluginCache {
	path: PathBuf,
}

impl HcPluginCache {
	pub fn new(path: &Path) -> Self {
		let plugins_path = pathbuf![path, "plugins"];
		Self { path: plugins_path }
	}

	/// `<path_to_plugin_cache>/<publisher>/<plugin_name>/<version>/<arch>`
	pub fn plugin_download_dir(
		&self,
		publisher: &PluginPublisher,
		name: &PluginName,
		version: &PluginVersion,
		arch: SupportedArch,
	) -> PathBuf {
		self.path
			.join(&publisher.0)
			.join(&name.0)
			.join(&version.0)
			.join(arch.to_string())
	}
}
