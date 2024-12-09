// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use pathbuf::pathbuf;
use std::{
	fs::OpenOptions,
	io::Write,
	path::{Path, PathBuf},
	str::FromStr,
};

use crate::task::manifest::{
	download_manifest::{DownloadManifest, DownloadManifestEntry},
	kdl::ToKdlNode,
};

/// Get the root of where download manifests live in the site dir of the local Hipcheck repo
pub fn site_plugins_dir() -> Result<PathBuf> {
	let root = crate::workspace::root()?;
	Ok(pathbuf![&root, "site/static/dl/plugin"])
}

pub fn get_download_manifest_path(publisher: &str, plugin: &str) -> Result<PathBuf> {
	let file_name = format!("{}.kdl", plugin);
	Ok(pathbuf![&site_plugins_dir()?, publisher, &file_name])
}

pub fn try_parse_download_manifest<P: AsRef<Path>>(path: P) -> Result<Vec<DownloadManifestEntry>> {
	fn inner(path: &Path) -> Result<Vec<DownloadManifestEntry>> {
		if !path.exists() {
			return Ok(vec![]);
		}
		let raw = std::fs::read_to_string(path)?;
		let manifest = DownloadManifest::from_str(&raw)?;
		Ok(manifest.entries)
	}
	inner(path.as_ref())
}

pub fn append_entries_to_file<'a, P: AsRef<Path>, T>(path: P, iter: T) -> Result<()>
where
	T: Iterator<Item = &'a DownloadManifestEntry>,
{
	fn inner(path: &Path, mut vec: Vec<&DownloadManifestEntry>) -> Result<()> {
		if vec.is_empty() {
			return Ok(());
		}
		// Sort so smallest version number is first
		vec.sort();
		let kdl_nodes = vec
			.into_iter()
			.map(ToKdlNode::to_kdl_node)
			.collect::<Result<Vec<_>>>()?;

		let mut out_string = "".to_owned();
		for node in kdl_nodes {
			let node_str = format!("\n{}\n", node);
			out_string.push_str(&node_str);
		}

		// If file exists, open in append mode, otherwise create new
		let mut f = OpenOptions::new().append(true).create(true).open(path)?;
		write!(f, "{}", out_string)?;

		Ok(())
	}
	inner(path.as_ref(), iter.collect())
}
