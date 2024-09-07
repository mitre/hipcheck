// SPDX-License-Identifier: Apache-2.0

//! Query group for fuzzing checks.

use super::github::GitHubProvider;
use crate::{
	config::ConfigSource,
	data::{get_fuzz_check, Fuzz},
	error::{Error, Result},
};

/// A query that provides a fuzz check
#[salsa::query_group(FuzzProviderStorage)]
pub trait FuzzProvider: ConfigSource + GitHubProvider {
	/// Returns the fuzz check results
	fn fuzz_check(&self) -> Result<Fuzz>;
}

/// Derived query implementations.  The returned `Fuzz` values
/// are wrapped in an `Rc` to keep cloning cheap and let other types
/// hold references to them.
fn fuzz_check(db: &dyn FuzzProvider) -> Result<Fuzz> {
	let repo_uri = db
		.url()
		.ok_or_else(|| Error::msg("unable to get repo url to check for fuzzing"))?;

	log::debug!("repo url {}", repo_uri);

	let token = db.github_api_token().ok_or_else(|| {
		Error::msg(
			"missing GitHub token with permissions for accessing public repository data in config",
		)
	})?;

	let fuzz = get_fuzz_check(&token, repo_uri)?;

	Ok(fuzz)
}
