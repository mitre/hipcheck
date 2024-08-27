// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};

use pathbuf::pathbuf;

pub struct HcPluginCache {
	path: PathBuf,
}

impl HcPluginCache {
	pub fn new(path: &Path) -> Self {
		let plugins_path = pathbuf![path, "plugins"];
		Self { path: plugins_path }
	}
}
