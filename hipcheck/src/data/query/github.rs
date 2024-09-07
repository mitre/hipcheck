// SPDX-License-Identifier: Apache-2.0

//! Query group for data pertaining to a remote GitHub source.

use crate::{
	error::{Error, Result},
	source::SourceQuery,
	target::KnownRemote,
};
use std::sync::Arc;

/// Queries about a remote GitHub source
#[salsa::query_group(GitHubProviderStorage)]
pub trait GitHubProvider: SourceQuery {
	/// Returns the repository owner
	fn owner(&self) -> Result<Arc<String>>;

	/// Returns the repository name
	fn repo(&self) -> Result<Arc<String>>;

	/// Returns the pull request number if this is a single pull
	/// request query. Returns an error otherwise.
	fn pull_request(&self) -> Result<u64>;
}

/// Derived query implementations.  Return values are wrapped in an
/// `Rc` to keep cloning cheap.
fn owner(db: &dyn GitHubProvider) -> Result<Arc<String>> {
	let remote = db.remote().ok_or(Error::msg("no remote for repo"))?;

	match remote.known_remote.as_ref() {
		Some(KnownRemote::GitHub { owner, .. }) => Ok(Arc::new(owner.to_string())),
		_ => Err(Error::msg(
			"unsupported remote host (supported: github.com)",
		)),
	}
}

fn repo(db: &dyn GitHubProvider) -> Result<Arc<String>> {
	let remote = db.remote().ok_or(Error::msg("no remote for repo"))?;

	match remote.known_remote.as_ref() {
		Some(KnownRemote::GitHub { repo, .. }) => Ok(Arc::new(repo.to_string())),
		_ => Err(Error::msg(
			"unsupported remote host (supported: github.com)",
		)),
	}
}

fn pull_request(_db: &dyn GitHubProvider) -> Result<u64> {
	Err(Error::msg("pull requests are no longer supported"))
}
