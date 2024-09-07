// SPDX-License-Identifier: Apache-2.0

use crate::context::Context as _;
use crate::error::Result;
use crate::hc_error;
use serde::de::DeserializeOwned;
use std::fs::{self, read_dir};
use std::ops::Not;
use std::path::{Path, PathBuf};

/// Read a file to a string.
pub fn read_string<P: AsRef<Path>>(path: P) -> Result<String> {
	fn inner(path: &Path) -> Result<String> {
		fs::read_to_string(path)
			.with_context(|| format!("failed to read as UTF-8 string '{}'", path.display()))
	}

	inner(path.as_ref())
}

/// Read file to a byte buffer.
pub fn read_bytes<P: AsRef<Path>>(path: P) -> Result<Vec<u8>> {
	fn inner(path: &Path) -> Result<Vec<u8>> {
		fs::read(path).with_context(|| format!("failed to read as bytes '{}'", path.display()))
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

/// Read file to a struct that can be deserialize from JSON format.
pub fn read_json<P: AsRef<Path>, T: DeserializeOwned>(path: P) -> Result<T> {
	let path = path.as_ref();
	let contents = read_bytes(path)?;
	serde_json::from_slice(&contents)
		.with_context(|| format!("failed to read as JSON '{}'", path.display()))
}

/// Create a directory and missing parents.
pub fn create_dir_all<P: AsRef<Path>>(path: P) -> Result<()> {
	fn inner(path: &Path) -> Result<()> {
		fs::create_dir_all(path)
			.with_context(|| format!("failed to create directory '{}'", path.display()))
	}

	inner(path.as_ref())
}

/// Check that a given path exists.
pub fn exists<P: AsRef<Path>>(path: P) -> Result<()> {
	fn inner(path: &Path) -> Result<()> {
		if path.exists().not() {
			Err(hc_error!(
				"'{}' not found at current directory",
				path.display()
			))
		} else {
			Ok(())
		}
	}

	inner(path.as_ref())
}

/// find the first file that has a given name
pub fn find_file_by_name<P: AsRef<Path>>(dir: P, file_name: &str) -> Result<PathBuf> {
	fn inner(dir: &Path, file_name: &str) -> Result<PathBuf> {
		if dir.is_dir() {
			for entry in read_dir(dir)
				.map_err(|e| hc_error!("Error [{}] reading {}", e, dir.to_string_lossy()))?
			{
				let entry = entry.map_err(|_| hc_error!("Failed to read entry"))?;
				let path = entry.path();
				if path.is_dir() {
					if let Ok(found) = find_file_by_name(&path, file_name) {
						return Ok(found);
					}
				} else if path.file_name().map_or(false, |name| name == file_name) {
					return Ok(path);
				}
			}
		}
		Err(hc_error!(
			"Could not find {} inside {}",
			file_name,
			dir.to_string_lossy()
		))
	}

	inner(dir.as_ref(), file_name)
}
