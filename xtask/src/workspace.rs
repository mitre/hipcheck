// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use anyhow::Result;
use std::path::Path;
use std::path::PathBuf;

/// Get the root directory of the workspace.
pub fn root() -> Result<PathBuf> {
	Path::new(&env!("CARGO_MANIFEST_DIR"))
		.ancestors()
		.nth(1)
		.ok_or_else(|| anyhow!("can't find cargo root"))
		.map(Path::to_path_buf)
}
