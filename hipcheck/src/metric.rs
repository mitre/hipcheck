// SPDX-License-Identifier: Apache-2.0

pub mod activity;
pub mod affiliation;
pub mod binary;
pub mod binary_detector;
pub mod churn;
pub mod commit_trust;
pub mod contributor_trust;
pub mod entropy;
pub mod fuzz;
pub mod identity;
pub mod linguist;
mod math;
#[allow(clippy::module_inception)]
pub mod metric;
pub mod module;
pub mod module_contributors;
pub mod review;
pub mod typo;

pub use metric::MetricProvider;
pub use metric::MetricProviderStorage;
