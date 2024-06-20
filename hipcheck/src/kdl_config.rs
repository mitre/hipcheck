use crate::util::fs::read_string;
use crate::Result;
use kdl::KdlDocument;
use std::path::Path;

/// Parse the KDL configuration file.
pub fn parse_kdl_config(path: &Path) -> Result<KdlDocument> {
	let contents = read_string(path)?;
	let document = contents.parse()?;
	Ok(document)
}
