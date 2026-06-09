//! Platform-specific helpers for inspecting file permissions.
//!
//! This crate provides safe APIs for permission checks that need platform-specific
//! implementation details. In particular, Windows ACL inspection requires FFI, so
//! this crate keeps that code out of callers such as `nv-server`.

use camino::{Utf8Path, Utf8PathBuf};
use std::{io, path::Path};

#[cfg(windows)]
mod windows;

/// Extension methods for checking whether file permissions are too broad.
pub trait FilePermissionsExt {
    /// Returns whether this file grants read access to a broad principal.
    ///
    /// On Windows, this inspects the file's discretionary access control list
    /// (DACL) and returns `true` when an allow entry grants generic read access
    /// to a broad built-in principal such as Everyone, Authenticated Users,
    /// Builtin Users, or Guests. A missing DACL is also treated as broadly
    /// readable because Windows interprets it as allowing access.
    ///
    /// On non-Windows platforms, this currently returns `Ok(false)`. Unix
    /// callers should use native mode-bit checks when they need Unix-specific
    /// owner, group, and other permission semantics.
    ///
    /// Returns an [`io::Error`] if the platform permission metadata cannot be
    /// read.
    fn allows_broad_read(&self) -> io::Result<bool>;
}

#[cfg(not(windows))]
impl FilePermissionsExt for Path {
    fn allows_broad_read(&self) -> io::Result<bool> {
        Ok(false)
    }
}

impl FilePermissionsExt for Utf8Path {
    fn allows_broad_read(&self) -> io::Result<bool> {
        self.as_std_path().allows_broad_read()
    }
}

impl FilePermissionsExt for Utf8PathBuf {
    fn allows_broad_read(&self) -> io::Result<bool> {
        self.as_path().allows_broad_read()
    }
}
