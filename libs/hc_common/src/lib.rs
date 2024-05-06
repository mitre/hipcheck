// SPDX-License-Identifier: Apache-2.0

//! Provides common access to third-party crates and other
//! functionality used widely throughout Hipcheck.

pub mod command_util;
pub mod config;
pub mod context;
pub mod error;
pub mod filesystem;
pub mod pathbuf;
pub mod report;
pub mod shell;
pub mod test_util;
#[cfg(test)]
mod tests;
mod try_any;
mod try_filter;
pub mod version;

pub use chrono;
pub use lazy_static;
pub use log;
pub use ordered_float;
pub use salsa;
pub use schemars;
pub use semver;
pub use serde_json;
pub use try_any::TryAny;
pub use try_filter::{FallibleFilter, TryFilter};
pub use url;
pub use which;

/// An `f64` that is never `NaN`.
pub type F64 = ordered_float::NotNan<f64>;

//Global variables for toml files per issue 157 config updates
pub const LANGS_FILE: &str = "Langs.toml";
pub const BINARY_CONFIG_FILE: &str = "Binary.toml";
pub const TYPO_FILE: &str = "Typos.toml";
pub const ORGS_FILE: &str = "Orgs.toml";
pub const HIPCHECK_TOML_FILE: &str = "Hipcheck.toml";

// Constants for exiting with error codes.
/// Indicates the program failed.
pub const EXIT_FAILURE: i32 = 1;

/// Indicates the program succeeded.
pub const EXIT_SUCCESS: i32 = 0;

//used in hc_session::pm and main.rs, global variables for hc check CheckKindHere node-ipc@9.2.1
pub enum CheckKind {
	Repo,
	Request,
	Patch,
	Maven,
	Npm,
	Pypi,
	Spdx,
}

impl CheckKind {
	pub const fn name(&self) -> &'static str {
		match self {
			CheckKind::Repo => "repo",
			CheckKind::Request => "request",
			CheckKind::Patch => "patch",
			CheckKind::Maven => "maven",
			CheckKind::Npm => "npm",
			CheckKind::Pypi => "pypi",
			CheckKind::Spdx => "spdx",
		}
	}
}
