// SPDX-License-Identifier: Apache-2.0

use crate::MetricProvider;
use hc_common::context::Context as _;
use hc_common::{error::Result, log};
use hc_data::git::Commit;
use serde::{self, Serialize};
use std::rc::Rc;

#[derive(Debug, Eq, PartialEq, Serialize)]
#[serde(crate = "self::serde")]
pub struct IdentityOutput {
	pub matches: Vec<Match>,
}

#[derive(Debug, Eq, PartialEq, Serialize)]
#[serde(crate = "self::serde")]
pub struct Match {
	pub commit: Rc<Commit>,
	pub identities_match: bool,
}

pub fn identity_metric(db: &dyn MetricProvider) -> Result<Rc<IdentityOutput>> {
	log::debug!("running identity metric");

	let commits = db.commits().context("failed to get commits")?;

	let mut matches = Vec::with_capacity(commits.len());

	for commit in commits.iter() {
		let commit_view = db
			.contributors_for_commit(Rc::clone(commit))
			.context("failed to get commits")?;

		let identities_match = commit_view.author == commit_view.committer;

		matches.push(Match {
			commit: Rc::clone(commit),
			identities_match,
		});
	}

	log::info!("completed identity metric");

	Ok(Rc::new(IdentityOutput { matches }))
}
