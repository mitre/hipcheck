// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context as _, Result};
use serde::de::DeserializeOwned;
use std::{fs, path::Path};

/// Read file to a byte buffer.
pub fn read_bytes<P: AsRef<Path>>(path: P) -> Result<Vec<u8>> {
	fn inner(path: &Path) -> Result<Vec<u8>> {
		fs::read(path).with_context(|| format!("failed to read as bytes '{}'", path.display()))
	}

	inner(path.as_ref())
}

/// Read file to a struct that can be deserialize from JSON format.
pub fn read_json<P: AsRef<Path>, T: DeserializeOwned>(path: P) -> Result<T> {
	let path = path.as_ref();
	let contents = read_bytes(path)?;
	serde_json::from_slice(&contents)
		.with_context(|| format!("failed to read as JSON '{}'", path.display()))
}
