// SPDX-License-Identifier: Apache-2.0

mod query;

pub use query::*;

use crate::{context::Context, error::Result, hc_error, util::fs::read_toml};
use content_inspector::{inspect, ContentType};
use serde::{de::Visitor, Deserialize, Deserializer};
use std::{
	fmt,
	fmt::Formatter,
	fs::File,
	io::{prelude::Read, BufReader},
	path::Path,
	result::Result as StdResult,
	sync::Arc,
};
use walkdir::{DirEntry, WalkDir};

#[derive(Debug, PartialEq, Eq)]
pub struct BinaryFileDetector {
	extensions: Vec<String>,
}

impl BinaryFileDetector {
	/// Constructs a new `BinaryFileDetector` from the `Binary.toml` file.
	pub fn load<P: AsRef<Path>>(binary_config_file: P) -> crate::error::Result<BinaryFileDetector> {
		fn inner(binary_config_file: &Path) -> crate::error::Result<BinaryFileDetector> {
			let extensions_file: ExtensionsFile = read_toml(binary_config_file)
				.context("failed to read binary type defintions from Binary config file")?;

			let extensions = extensions_file.into_extensions();

			Ok(BinaryFileDetector { extensions })
		}

		inner(binary_config_file.as_ref())
	}

	/// Determines if a binary file matches a known file extension.
	///
	/// A match is assumed if an extension is not present.
	pub fn is_likely_binary_file<P: AsRef<Path>>(&self, file_name: P) -> bool {
		fn inner(binary_file_detector: &BinaryFileDetector, file_name: &Path) -> bool {
			let extension = match file_name.extension() {
				Some(e) => format!(".{}", e.to_string_lossy()),
				None => return true,
			};
			for ext in &binary_file_detector.extensions {
				if *ext == extension {
					return true;
				}
			}
			false
		}
		inner(self, file_name.as_ref())
	}
}

#[derive(Debug, Deserialize)]
struct ExtensionsFile {
	formats: Vec<BinaryExtensions>,
}

#[derive(Debug, Deserialize)]
struct BinaryExtensions {
	#[serde(default = "missing_bin_type")]
	r#type: BinaryType,
	extensions: Option<Vec<String>>,
}

impl ExtensionsFile {
	/// Collects the known file extensions from Binary.toml
	fn into_extensions(self) -> Vec<String> {
		let mut result = Vec::new();
		for file_format in self.formats {
			if matches!(
				file_format.r#type,
				BinaryType::Object | BinaryType::Combination | BinaryType::Executable
			) {
				match file_format.extensions {
					None => continue,
					Some(mut extensions) => result.extend(extensions.drain(0..)),
				}
			}
		}
		result
	}
}

#[derive(Debug)]
enum BinaryType {
	Object,
	Executable,
	Combination,
	Missing,
}

fn missing_bin_type() -> BinaryType {
	BinaryType::Missing
}

impl<'de> Deserialize<'de> for BinaryType {
	fn deserialize<D>(deserializer: D) -> StdResult<Self, D::Error>
	where
		D: Deserializer<'de>,
	{
		deserializer.deserialize_str(BinaryTypeVisitor)
	}
}

struct BinaryTypeVisitor;

impl<'de> Visitor<'de> for BinaryTypeVisitor {
	type Value = BinaryType;
	fn expecting(&self, f: &mut Formatter) -> fmt::Result {
		write!(f, "'executable', 'object', or 'combination'")
	}

	fn visit_str<E>(self, value: &str) -> StdResult<Self::Value, E>
	where
		E: serde::de::Error,
	{
		match value {
			"combination" => Ok(BinaryType::Combination),
			"object" => Ok(BinaryType::Object),
			"executable" => Ok(BinaryType::Executable),
			_ => Err(serde::de::Error::custom("unknown binary format")),
		}
	}
}

/// Determines whether a DirEntry is a hidden file/directory.
///
/// This is a Unix-style determination.
fn is_hidden(entry: &DirEntry) -> bool {
	entry
		.file_name()
		.to_str()
		.map(|s| s.starts_with('.'))
		.unwrap_or(false)
}

/// Fetches all files from `dir`.
fn fetch_entries(dir: &Path) -> Result<Vec<DirEntry>> {
	let walker = WalkDir::new(dir).into_iter();
	let mut entries: Vec<DirEntry> = Vec::new();
	for entry in walker.filter_entry(|e| !is_hidden(e)) {
		entries.push(entry?)
	}
	Ok(entries)
}

/// Searches `dir` for any binary files and records their paths as Strings.
pub fn detect_binary_files(dir: &Path) -> Result<Vec<Arc<String>>> {
	let path_entries = fetch_entries(dir)?;
	let mut possible_binary: Vec<Arc<String>> = Vec::new();

	// Inspect the first 4K of each file for telltale signs of binary data.
	// Store a String of each Path that leads to a binary file.
	const SAMPLE_SIZE: u64 = 4096;
	for entry in path_entries {
		// Skip directories, as they are neither text nor binary.
		if entry.path().is_dir() {
			continue;
		}

		let working_file = File::open(entry.path())?;
		let reader = BufReader::new(working_file);
		let mut contents: Vec<u8> = Vec::new();
		let _bytes_read = reader.take(SAMPLE_SIZE).read_to_end(&mut contents)?;
		if inspect(&contents) == ContentType::BINARY {
			possible_binary.push(match entry.path().strip_prefix(dir)?.to_str() {
				Some(p) => Arc::new(String::from(p)),
				None => {
					return Err(hc_error!(
						"path was not valid unicode: '{}'",
						entry.path().display()
					))
				}
			});
		}
	}

	Ok(possible_binary)
}
