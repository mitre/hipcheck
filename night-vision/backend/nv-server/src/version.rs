//! Helpers for getting version information for the Night Vision server.

/// Get the short version string, including just the pkg version and short commit hash.
pub fn get_version() -> String {
    let commit_info = CommitInfo::load();

    let mut s = env!("CARGO_PKG_VERSION").to_string();

    if let Some(short_hash) = &commit_info.short_hash {
        s.push_str(" (commit hash ");
        s.push_str(short_hash);
        s.push(')');
    }

    s
}

/// Get the long version string, including the pkg version, full commit hash, and commit date.
pub fn get_long_version() -> String {
    let commit_info = CommitInfo::load();

    let mut s = env!("CARGO_PKG_VERSION").to_string();

    if let Some(hash) = &commit_info.hash
        && let Some(date) = &commit_info.date
    {
        s.push_str(" (commit hash ");
        s.push_str(hash);
        s.push_str(" on ");
        s.push_str(date);
        s.push(')');
    }

    s
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
