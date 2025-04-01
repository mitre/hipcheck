// SPDX-License-Identifier: Apache-2.0

mod download_manifest;
mod kdl;
mod local;
mod remote;
mod util;

use download_manifest::DownloadManifestEntry;

use anyhow::Result;
use std::collections::HashSet;

#[allow(unused)]
pub use kdl::ParseKdlNode;

pub fn run() -> Result<()> {
	let api_token = std::env::var("HC_GITHUB_TOKEN")?;
	let releases = remote::get_hipcheck_plugin_releases(&api_token)?;

	for (name, remote_entries) in releases.0 {
		// We know all releases off the hipcheck github repo are mitre-published
		let local_manifest_path = local::get_download_manifest_path("mitre", &name)?;
		let local_entries = local::try_parse_download_manifest(&local_manifest_path)?;

		let remote_set = HashSet::<DownloadManifestEntry>::from_iter(remote_entries.into_iter());
		let local_set = HashSet::<DownloadManifestEntry>::from_iter(local_entries.into_iter());

		let diff = remote_set.difference(&local_set);
		let diff_vec = diff.collect::<Vec<_>>();

		if diff_vec.is_empty() {
			continue;
		}
		log::info!(
			"Updating download manifest for '{}' with {} new entries",
			name,
			diff_vec.len()
		);

		local::append_entries_to_file(local_manifest_path, diff_vec.into_iter())?;
	}

	Ok(())
}
