// SPDX-License-Identifier: Apache-2.0

use crate::context::Context as _;
use crate::data::git::Commit;
use crate::error::Result;
use crate::metric::MetricProvider;
use serde::Serialize;
use std::sync::Arc;

#[derive(Debug, Eq, PartialEq, Serialize)]
pub struct IdentityOutput {
	pub matches: Vec<Match>,
}

#[derive(Debug, Eq, PartialEq, Serialize)]
pub struct Match {
	pub commit: Arc<Commit>,
	pub identities_match: bool,
}

pub fn identity_metric(db: &dyn MetricProvider) -> Result<Arc<IdentityOutput>> {
	log::debug!("running identity metric");

	let commits = db.commits().context("failed to get commits")?;

	let mut matches = Vec::with_capacity(commits.len());

	for commit in commits.iter() {
		let commit_view = db
			.contributors_for_commit(Arc::clone(commit))
			.context("failed to get commits")?;

		let identities_match = commit_view.author == commit_view.committer;

		matches.push(Match {
			commit: Arc::clone(commit),
			identities_match,
		});
	}

	log::info!("completed identity metric");

	Ok(Arc::new(IdentityOutput { matches }))
}
