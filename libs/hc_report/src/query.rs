// SPDX-License-Identifier: Apache-2.0

//! Query groups for how Hipcheck reports and dumps session results.

pub use hc_version::VersionQuery;

use crate::Format;
use hc_common::{chrono::prelude::*, salsa};

/// Queries for how Hipcheck reports session results
#[salsa::query_group(ReportParamsStorage)]
pub trait ReportParams: VersionQuery {
	/// Returns the time the current Hipcheck session started
	#[salsa::input]
	fn started_at(&self) -> DateTime<FixedOffset>;

	/// Returns the format of the final report
	#[salsa::input]
	fn format(&self) -> Format;
}
