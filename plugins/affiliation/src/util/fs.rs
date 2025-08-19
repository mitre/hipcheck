// SPDX-License-Identifier: Apache-2.0

//! General shared functions for interacting with the file system

use anyhow::{Context as _, Result, anyhow};
use std::{fs, ops::Not, path::Path};

/// Read a file to a string.
pub fn read_string<P: AsRef<Path>>(path: P) -> Result<String> {
	fn inner(path: &Path) -> Result<String> {
		fs::read_to_string(path)
			.with_context(|| format!("failed to read as UTF-8 string '{}'", path.display()))
	}

	inner(path.as_ref())
}

/// Check that a given path exists.
pub fn exists<P: AsRef<Path>>(path: P) -> Result<()> {
	fn inner(path: &Path) -> Result<()> {
		if path.exists().not() {
			Err(anyhow!(
				"'{}' not found at current directory",
				path.display()
			))
		} else {
			Ok(())
		}
	}

	inner(path.as_ref())
}
