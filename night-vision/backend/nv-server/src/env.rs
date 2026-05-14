//! Provides an interface for accessing environment variables.

use std::{fmt::Display, ops::Deref};

/// Provides access to environment variables.
pub struct Env {
    /// Information about the current commit when nv-server was built.
    commit_info: CommitInfo,
    /// The name of the nv-server binary.
    bin_name: BinName,
}

impl Env {
    /// Load environment information.
    pub fn load() -> Self {
        Env {
            commit_info: CommitInfo::load(),
            bin_name: BinName::load(),
        }
    }

    /// Get the short version string, including just the pkg version and short commit hash.
    pub fn bin_short_version(&self) -> String {
        let mut s = env!("CARGO_PKG_VERSION").to_string();

        if let Some(short_hash) = &self.commit_info.short_hash {
            s.push_str(" (commit hash ");
            s.push_str(short_hash);
            s.push(')');
        }

        s
    }

    /// Get the long version string, including the pkg version, full commit hash, and commit date.
    pub fn bin_long_version(&self) -> String {
        let mut s = env!("CARGO_PKG_VERSION").to_string();

        if let Some(hash) = &self.commit_info.hash
            && let Some(date) = &self.commit_info.date
        {
            s.push_str(" (commit hash ");
            s.push_str(hash);
            s.push_str(" on ");
            s.push_str(date);
            s.push(')');
        }

        s
    }

    /// Get the name of the binary.
    pub fn bin_name(&self) -> String {
        self.bin_name.to_string()
    }
}

/// Represents the commit information for the Night Vision server.
struct CommitInfo {
    /// The full-length hash of the commit.
    hash: Option<String>,
    /// The short hash of the commit.
    short_hash: Option<String>,
    /// The date of the commit.
    date: Option<String>,
}

impl CommitInfo {
    /// Loads the commit information from environment variables.
    fn load() -> Self {
        Self {
            hash: option_env!("NV_BUILD_COMMIT_HASH").map(ToString::to_string),
            short_hash: option_env!("NV_BUILD_COMMIT_SHORT_HASH").map(ToString::to_string),
            date: option_env!("NV_BUILD_COMMIT_DATE").map(ToString::to_string),
        }
    }
}

/// The name of the binary, derived from the crate name.
#[derive(Debug)]
struct BinName(String);

impl BinName {
    /// Get the crate name, replacing underscores with hyphens.
    fn load() -> Self {
        BinName(env!("CARGO_CRATE_NAME").replace("_", "-").to_string())
    }
}

impl Display for BinName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Deref for BinName {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}
