// SPDX-License-Identifier: Apache-2.0

use crate::context::Context as _;
use crate::{
	error::Result,
	hc_error,
	serde::{de::DeserializeOwned, Serialize},
	serde_json,
};
use std::fs::{self, File};
use std::ops::Not;
use std::path::Path;

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

/// Write to a file.
#[allow(dead_code)]
pub fn write<P: AsRef<Path>, C: AsRef<[u8]>>(path: P, contents: C) -> Result<()> {
	fn inner(path: &Path, contents: &[u8]) -> Result<()> {
		fs::write(path, contents).with_context(|| format!("failed to write '{}'", path.display()))
	}

	inner(path.as_ref(), contents.as_ref())
}

/// Write JSON to a file.
pub fn write_json<P: AsRef<Path>, T: ?Sized + Serialize>(path: P, value: &T) -> Result<()> {
	// The non-generic inner function (NGIF) trick is useless here, since T would still be generic,
	// so instead we do this conversion up-front to avoid borrowing issues with the `with_context`
	// closure taking ownership of `path`.
	let path = path.as_ref();
	let mut file = create(path)?;
	serde_json::to_writer_pretty(&mut file, value)
		.with_context(|| format!("failed to write JSON '{}'", path.display()))
}

/// Create a new file.
pub fn create<P: AsRef<Path>>(path: P) -> Result<File> {
	fn inner(path: &Path) -> Result<File> {
		File::create(path).with_context(|| format!("failed to create file '{}'", path.display()))
	}

	inner(path.as_ref())
}

/// Open an existing file.
#[allow(dead_code)]
pub fn open<P: AsRef<Path>>(path: P) -> Result<File> {
	fn inner(path: &Path) -> Result<File> {
		File::open(path).with_context(|| format!("failed to open file '{}'", path.display()))
	}

	inner(path.as_ref())
}

/// Create a directory and missing parents.
#[allow(dead_code)]
pub fn create_dir_all<P: AsRef<Path>>(path: P) -> Result<()> {
	fn inner(path: &Path) -> Result<()> {
		fs::create_dir_all(path)
			.with_context(|| format!("failed to create directory '{}'", path.display()))
	}

	inner(path.as_ref())
}

/// Remove a directory and any children.
#[allow(dead_code)]
pub fn remove_dir_all<P: AsRef<Path>>(path: P) -> Result<()> {
	fn inner(path: &Path) -> Result<()> {
		fs::remove_dir_all(path)
			.with_context(|| format!("failed to remove directory '{}'", path.display()))
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
