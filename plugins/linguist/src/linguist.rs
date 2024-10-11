// SPDX-License-Identifier: Apache-2.0

use crate::fs::read_toml;
use anyhow::{Context as _, Result};
use serde::{de::Visitor, Deserialize, Deserializer};
use std::{convert::AsRef, fmt, fmt::Formatter, path::Path, result::Result as StdResult};

/// Detects whether a file name is a likely source code file.
#[derive(Debug, PartialEq, Eq)]
pub struct SourceFileDetector {
	extensions: Vec<String>,
}

impl SourceFileDetector {
	#[cfg(test)]
	pub fn new(raw_exts: Vec<&str>) -> Self {
		let extensions = raw_exts.into_iter().map(str::to_owned).collect();
		SourceFileDetector { extensions }
	}

	/// Constructs a new `SourceFileDetector` from the `languages.yml` file.
	pub fn load<P: AsRef<Path>>(langs_file: P) -> Result<SourceFileDetector> {
		fn inner(langs_file: &Path) -> Result<SourceFileDetector> {
			// Load the file and parse it.
			let language_file: LanguageFile = read_toml(langs_file)
				.context("failed to read language definitions from langs file")?;

			// Get the list of extensions from it.
			let extensions = language_file.into_extensions();

			// Return the initialized detector.
			Ok(SourceFileDetector { extensions })
		}

		inner(langs_file.as_ref())
	}

	/// Checks whether a given file is a likely source file based on its file
	/// extension.
	pub fn is_likely_source_file<P: AsRef<Path>>(&self, file_name: P) -> bool {
		fn inner(source_file_detector: &SourceFileDetector, file_name: &Path) -> bool {
			let extension = match file_name.extension() {
				// Convert &OsStr to Cow<&str> and prepend a period to match the file
				Some(e) => format!(".{}", e.to_string_lossy()),
				// If we can't find an extension, include the file.
				None => return true,
			};

			for ext in &source_file_detector.extensions {
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
struct LanguageFile {
	languages: Vec<LanguageExtensions>,
}

#[derive(Debug, Deserialize)]
struct LanguageExtensions {
	#[serde(default = "missing_lang_type")]
	r#type: LanguageType,
	extensions: Option<Vec<String>>,
}

impl LanguageFile {
	fn into_extensions(self) -> Vec<String> {
		let mut result = Vec::new();

		for language in self.languages {
			if matches!(language.r#type, LanguageType::Programming) {
				match language.extensions {
					None => continue,
					Some(mut extensions) => result.extend(extensions.drain(0..)),
				}
			}
		}

		log::trace!("linguist known code extensions [exts='{:#?}']", result);

		result
	}
}

#[derive(Debug)]
enum LanguageType {
	Data,
	Programming,
	Markup,
	Prose,
	Missing,
}

fn missing_lang_type() -> LanguageType {
	LanguageType::Missing
}

impl<'de> Deserialize<'de> for LanguageType {
	fn deserialize<D>(deserializer: D) -> StdResult<Self, D::Error>
	where
		D: Deserializer<'de>,
	{
		deserializer.deserialize_str(LanguageTypeVisitor)
	}
}

struct LanguageTypeVisitor;

impl<'de> Visitor<'de> for LanguageTypeVisitor {
	type Value = LanguageType;

	fn expecting(&self, f: &mut Formatter) -> fmt::Result {
		write!(f, "'data', 'programming', 'markup', or 'prose'")
	}

	fn visit_str<E>(self, value: &str) -> StdResult<Self::Value, E>
	where
		E: serde::de::Error,
	{
		match value {
			"data" => Ok(LanguageType::Data),
			"programming" => Ok(LanguageType::Programming),
			"markup" => Ok(LanguageType::Markup),
			"prose" => Ok(LanguageType::Prose),
			_ => Err(serde::de::Error::custom("unknown language type")),
		}
	}
}
