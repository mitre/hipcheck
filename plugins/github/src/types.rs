// SPDX-License-Identifier: Apache-2.0

use chrono::{DateTime, Utc};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct GitHubPullRequest {
	pub number: u64,
	pub reviews: u64,
	pub date_merged: DateTime<Utc>,
}
