// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, Context as _, Result};
use serde::de::DeserializeOwned;
use std::{fs, path::Path};

/// Read a file to a string.
pub fn read_string<P: AsRef<Path>>(path: P) -> Result<String> {
	fn inner(path: &Path) -> Result<String> {
		fs::read_to_string(path)
			.with_context(|| format!("failed to read as UTF-8 string '{}'", path.display()))
	}

	inner(path.as_ref())
}

/// Read file to a struct that can be deserialized from TOML format.
pub fn read_toml<P: AsRef<Path>, T: DeserializeOwned>(path: P) -> Result<T> {
	let path = path.as_ref();
	let contents = read_string(path)?;
	toml::de::from_str(&contents)
		.with_context(|| format!("failed to read as TOML '{}'", path.display()))
}

/// Check that a given path exists.
pub fn exists<P: AsRef<Path>>(path: P) -> Result<()> {
	fn inner(path: &Path) -> Result<()> {
		if !path.exists() {
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
