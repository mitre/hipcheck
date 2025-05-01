// SPDX-License-Identifier: Apache-2.0

use crate::{
	error::{Context, Result},
	util::{command::DependentProgram, git::get_git_version},
};
use semver::Version;
use std::{sync::Arc, sync::OnceLock};

/// struct which holds the version information for binaries relevant to `hc` used in this run
struct SoftwareVersions {
	hc: Arc<String>,
	git: Arc<String>,
}

/// Holds the versions of software used in a particular run of `hc`
static HC_VERSIONS: OnceLock<SoftwareVersions> = OnceLock::new();

/// Determine the version of all dependent programs, as well as `hc` itself
pub fn init_software_versions() -> Result<()> {
	let git_version = get_git_version()?;
	DependentProgram::Git.check_version(&git_version)?;

	let raw_hc_version = env!("CARGO_PKG_VERSION", "can't find Hipcheck package version");
	let hc_version = parse_hc_version(raw_hc_version)?;

	HC_VERSIONS.get_or_init(|| SoftwareVersions {
		hc: Arc::new(hc_version),
		git: Arc::new(git_version),
	});
	Ok(())
}

#[allow(unused)]
/// retrieve the version of `git` used in this run of `hc`
pub fn git_version() -> Arc<String> {
	HC_VERSIONS
		.get()
		.expect("'HC_VERSIONS' not initialized")
		.git
		.clone()
}

/// retrieve the version of `hc` used in this run of `hc`
pub fn hc_version() -> Arc<String> {
	HC_VERSIONS
		.get()
		.expect("'HC_VERSIONS' not initialized")
		.hc
		.clone()
}

/// Parse the version of `hc` to ensure it is semver compliant
pub fn parse_hc_version(version: &str) -> Result<String> {
	let hc_version = Version::parse(version)
		.context("can't parse version in Cargo.toml")?
		.to_string();
	log::debug!("detected Hipcheck version [version='{:?}']", hc_version);
	Ok(hc_version)
}
