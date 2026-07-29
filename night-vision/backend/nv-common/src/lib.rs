//! Shared backend infrastructure for Night Vision binaries.
//!
//! This crate holds backend code that is needed by both `nv-server` and
//! command-line tools such as `nvdb`. Keeping configuration loading, database
//! setup, runtime construction, and other shared helpers here avoids duplicating
//! startup behavior across binaries and keeps server-specific code out of tools.

#![deny(unsafe_code)]

pub mod config;
pub mod cve;
pub mod db;
pub mod error;
pub mod log;
pub mod npm;
pub mod rt;
pub mod secret;

#[cfg(test)]
mod test_util;
