// SPDX-License-Identifier: Apache-2.0

use crate::{error::Error, plugin::PluginVersionReq};
use semver::Version;

// Pre-process the plugin version requirement string, then instantiate and return a Result containing a PluginVersionReq.
// If the version_req string is not in the expected Semver Version Requirement format, a "=" will be prepended to it to
// make the Semver VersionReq object treat the version as an exact version requirement
pub fn pre_process_plugin_version_req(version_req: &str) -> Result<PluginVersionReq, Error> {
	if Version::parse(version_req).is_ok() {
		PluginVersionReq::new(&format!("={}", version_req))
	} else {
		PluginVersionReq::new(version_req)
	}
}
