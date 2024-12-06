// SPDX-License-Identifier: Apache-2.0

pub mod download_manifest;
mod kdl;
pub mod remote;
mod util;

use anyhow::Result;

pub fn run() -> Result<()> {
	let api_token = std::env::var("HC_GITHUB_TOKEN")?;
	let releases = remote::get_hipcheck_releases(&api_token)?;

	for (name, release) in releases.0 {
		println!("{name}: {release:?}");
	}

	Ok(())
}
