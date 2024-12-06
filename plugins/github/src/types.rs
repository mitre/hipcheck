// SPDX-License-Identifier: Apache-2.0

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct GitHubPullRequest {
	pub number: u64,
	pub reviews: u64,
}
