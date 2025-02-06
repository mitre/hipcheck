// SPDX-License-Identifier: Apache-2.0
use crate::error::{Context, Result};
use crate::hc_error;
use crate::util::fs::read_kdl;
use content_inspector::{inspect, ContentType};
use hipcheck_kdl::kdl::{KdlDocument, KdlNode};
use hipcheck_kdl::ParseKdlNode;
use miette::Report;
use serde::{de::Visitor, Deserialize, Deserializer};
use std::{
	fmt,
	fmt::Formatter,
	fs::File,
	io::{prelude::Read, BufReader},
	path::{Path, PathBuf},
	result::Result as StdResult,
	str::FromStr,
};
use walkdir::{DirEntry, WalkDir};

#[derive(Debug, PartialEq, Eq)]
pub struct BinaryFileDetector {
	extensions: Vec<String>,
}

impl BinaryFileDetector {
	/// Constructs a new `BinaryFileDetector` from the `Binary.kdl` file.
	pub fn load<P: AsRef<Path>>(binary_config_file: P) -> crate::error::Result<BinaryFileDetector> {
		fn inner(binary_config_file: &Path) -> crate::error::Result<BinaryFileDetector> {
			let extensions_file = read_kdl(binary_config_file).with_context(|| {
				format!(
					"failed to read binary type definitions from Binary config file at path {:?}",
					binary_config_file
				)
			})?;

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
pub struct ExtensionsFile {
	formats: Vec<BinaryExtensions>,
}

impl FromStr for ExtensionsFile {
	type Err = crate::error::Error;

	fn from_str(s: &str) -> StdResult<Self, Self::Err> {
		let document = KdlDocument::from_str(s)
			// Print miette::Report with Debug for full help text
			.map_err(|e| hc_error!("File doesn't parse as valid KDL:\n{:?}", Report::from(e)))?;
		let mut formats = vec![];
		for node in document.nodes() {
			if let Some(entry) = BinaryExtensions::parse_node(node) {
				formats.push(entry);
			} else {
				return Err(hc_error!("Error parsing Binary entry: {}", node));
			}
		}
		Ok(Self { formats })
	}
}

impl ExtensionsFile {
	/// Collects the known file extensions from Binary.kdl
	fn into_extensions(self) -> Vec<String> {
		let mut result = Vec::new();
		for file_format in self.formats {
			if matches!(
				file_format.r#type,
				BinaryType::Object | BinaryType::Combination | BinaryType::Executable
			) {
				let mut extensions = file_format.extensions;
				result.extend(extensions.drain(0..));
			}
		}
		result
	}
}

#[derive(Debug, Deserialize)]
struct BinaryExtensions {
	#[serde(default = "missing_bin_type")]
	r#type: BinaryType,
	extensions: Vec<String>,
}

impl ParseKdlNode for BinaryExtensions {
	fn kdl_key() -> &'static str {
		"format"
	}

	fn parse_node(node: &KdlNode) -> Option<Self> {
		if node.name().to_string().as_str() != Self::kdl_key() {
			return None;
		}

		let binary_type = node.get("type")?.as_string()?;

		let r#type = match binary_type {
			"object" => Some(BinaryType::Object),
			"executable" => Some(BinaryType::Executable),
			"combination" => Some(BinaryType::Combination),
			_ => None,
		}?;

		let mut extensions_from_node = Vec::new();
		for node in node.children()?.nodes() {
			if node.name().to_string().as_str() == "extensions" {
				for entry in node.entries() {
					extensions_from_node.push(entry.value().as_string()?.to_string());
				}
			}
		}

		Some(Self {
			r#type,
			extensions: extensions_from_node,
		})
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

impl Visitor<'_> for BinaryTypeVisitor {
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
pub fn detect_binary_files(dir: &Path) -> Result<Vec<PathBuf>> {
	let path_entries = fetch_entries(dir)?;
	let mut possible_binary: Vec<PathBuf> = Vec::new();

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
			possible_binary.push(entry.path().strip_prefix(dir)?.into());
		}
	}

	Ok(possible_binary)
}
