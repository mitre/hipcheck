// SPDX-License-Identifier: Apache-2.0

use hc_common::{error::Result, hc_error};
use std::path::{Path, PathBuf};

pub fn root() -> Result<PathBuf> {
	Path::new(&env!("CARGO_MANIFEST_DIR"))
		.ancestors()
		.nth(1)
		.ok_or_else(|| hc_error!("can't find cargo root"))
		.map(Path::to_path_buf)
}
