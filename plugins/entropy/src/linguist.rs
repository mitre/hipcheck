// SPDX-License-Identifier: Apache-2.0

use crate::error::{Context, Result};
use crate::hc_error;
use crate::util::fs::read_kdl;
use crate::util::kdl::ParseKdlNode;
use kdl::KdlDocument;
use serde::{de::Visitor, Deserialize, Deserializer};
use std::{
	convert::AsRef, fmt, fmt::Formatter, path::Path, result::Result as StdResult, str::FromStr,
};

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
			let language_file: LanguageFile = read_kdl(langs_file)
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
pub struct LanguageFile {
	languages: Vec<LanguageExtensions>,
}

impl FromStr for LanguageFile {
	type Err = crate::error::Error;

	fn from_str(s: &str) -> StdResult<Self, Self::Err> {
		let document = KdlDocument::from_str(s)
			.map_err(|e| hc_error!("Error parsing Langs.kdl file: {}", e.to_string()))?;
		let mut languages = vec![];
		for node in document.nodes() {
			if let Some(entry) = LanguageExtensions::parse_node(node) {
				languages.push(entry);
			} else {
				return Err(hc_error!("Error parsing Language entry: {}", node));
			}
		}
		Ok(Self { languages })
	}
}

impl LanguageFile {
	fn into_extensions(self) -> Vec<String> {
		let mut result = Vec::new();

		for language in self.languages {
			if matches!(language.r#type, LanguageType::Programming) {
				let mut extensions = language.extensions;
				result.extend(extensions.drain(0..));
			}
		}

		result
	}
}

#[derive(Debug, Deserialize)]
struct LanguageExtensions {
	#[serde(default = "missing_lang_type")]
	r#type: LanguageType,
	extensions: Vec<String>,
}

impl ParseKdlNode for LanguageExtensions {
	fn kdl_key() -> &'static str {
		"language"
	}

	fn parse_node(node: &kdl::KdlNode) -> Option<Self> {
		if node.name().to_string().as_str() != Self::kdl_key() {
			return None;
		}

		let lanugage_type = node.get("type")?.value().as_string()?;

		let r#type = match lanugage_type {
			"data" => Some(LanguageType::Data),
			"programming" => Some(LanguageType::Programming),
			"markup" => Some(LanguageType::Markup),
			"prose" => Some(LanguageType::Prose),
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

impl Visitor<'_> for LanguageTypeVisitor {
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
