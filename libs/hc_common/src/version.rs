// SPDX-License-Identifier: Apache-2.0

use crate::{context::Context, error::Result, log, salsa, semver::Version};
use std::ops::Not as _;
use std::rc::Rc;

/// Query the environment to identify the proper version string.
pub fn get_version(raw_version: &str) -> Result<String> {
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

/// Queries for current versions of Hipcheck and tool dependencies
#[salsa::query_group(VersionQueryStorage)]
pub trait VersionQuery: salsa::Database {
	/// Returns the current Hipcheck version
	#[salsa::input]
	fn hc_version(&self) -> Rc<String>;

	/// Returns the version of npm currently running on user's machine
	#[salsa::input]
	fn npm_version(&self) -> Rc<String>;

	/// Returns the version of eslint currently running on user's machine
	#[salsa::input]
	fn eslint_version(&self) -> Rc<String>;

	/// Returns the version of git currently running on user's machine
	#[salsa::input]
	fn git_version(&self) -> Rc<String>;
}
