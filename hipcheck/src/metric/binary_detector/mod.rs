// SPDX-License-Identifier: Apache-2.0

mod query;

pub use query::*;

use crate::{
	error::Context,
	util::fs::read_toml,
};

use serde::{de::Visitor, Deserialize, Deserializer};
use std::{
	fmt,
	fmt::Formatter,
	path::Path,
	result::Result as StdResult,
};
 
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
