//! CVE List ingestion support.

pub mod git;
pub mod record;
pub mod repository;
pub mod storage;
pub mod sync;

#[cfg(test)]
mod integration_tests;
