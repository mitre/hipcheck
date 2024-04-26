// SPDX-License-Identifier: Apache-2.0

mod query;

pub use query::*;

use hc_common::{
	context::{Context, Result},
	log,
	semver::Version,
};
use std::ops::Not as _;

pub fn get_version(raw_version: &str) -> Result<String> {
	// Queries the environment to identify the proper version string.
	//
	// Basic algorithm:
	//     1. Check the version number in `Cargo.toml`.
	//     2. If it's an "alpha" release, then in addition to printing
	//        the version number, we should also capture the current "HEAD"
	//        commit of the repository and output the version identifier
	//        as `<version number> (<HEAD commit>)`

	let version = Version::parse(raw_version).context("can't parse version in Cargo.toml")?;

	log::debug!("detected Hipcheck version [version='{:?}']", version);

	if version.pre.as_str().starts_with("alpha") {
		let head = option_env!("HC_HEAD_COMMIT").unwrap_or("");

		if head.is_empty().not() {
			return Ok(format!("{} ({})", raw_version, head));
		}
	}

	Ok(raw_version.to_string())
}
