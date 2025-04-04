// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, Context as _, Result};
use hipcheck_kdl::kdl::KdlDocument;
use hipcheck_kdl::{extract_data, ParseKdlNode};
use miette::Report;
use std::{fs, path::Path, str::FromStr};

/// Read a file to a string.
pub fn read_string<P: AsRef<Path>>(path: P) -> Result<String> {
	fn inner(path: &Path) -> Result<String> {
		fs::read_to_string(path)
			.with_context(|| format!("failed to read as UTF-8 string '{}'", path.display()))
	}

	inner(path.as_ref())
}

/// Read file to a struct that can be deserialized from kdl format.
pub fn read_kdl<P: AsRef<Path>, T: ParseKdlNode>(path: P) -> Result<T> {
	let path = path.as_ref();
	let contents = read_string(path)?;
	// Print miette::Report with Debug for full help text
	let document = KdlDocument::from_str(&contents)
		.map_err(|e| anyhow!("File doesn't parse as valid KDL:\n{:?}", Report::from(e)))?;
	let nodes = document.nodes();
	extract_data(nodes).ok_or(anyhow!("Could not parse typo KDL 'format'"))
}

/// Check that a given path exists.
pub fn exists<P: AsRef<Path>>(path: P) -> Result<()> {
	fn inner(path: &Path) -> Result<()> {
		if !path.exists() {
			Err(anyhow!("'{}' not found", path.display()))
		} else {
			Ok(())
		}
	}

	inner(path.as_ref())
}
