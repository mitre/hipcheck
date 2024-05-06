// SPDX-License-Identifier: Apache-2.0

use crate::analysis::MetricProvider;
use crate::context::Context as _;
use crate::error::Result;
use std::collections::HashMap;
use std::rc::Rc;

pub const TRUST_PHASE: &str = "commit trust";

#[derive(Debug, Eq, PartialEq)]
pub struct CommitTrustOutput {
	pub commit_counts_in_period: Rc<HashMap<String, bool>>,
}

//Add metric to check if a commit is trusted based on whether its author and committer are trusted (depends on trust metric)
pub fn commit_trust_metric(db: &dyn MetricProvider) -> Result<Rc<CommitTrustOutput>> {
	log::debug!("running {} metric", TRUST_PHASE);

	let contributor_trust_map = db
		.contributor_trust_metric()
		.context("unable to get contributor trust metric for commit trust")?;

	// Get the commits for the source.
	let commits = db.commits().context("unable to get commits")?;

	let mut trust_map: HashMap<String, bool> = HashMap::new();

	let value_threshold = db.contributor_trust_value_threshold() as u32;

	for commit in commits.iter() {
		// Check if a commit matches contributor trust map
		let msg = format!(
			"failed to get contributor commit view for {} metric",
			TRUST_PHASE
		);
		let commit_view = db.contributors_for_commit(Rc::clone(commit)).context(msg)?;

		for (email, count) in contributor_trust_map.contributor_counts_in_period.as_ref() {
			log::debug!(
				"contributor_trust_map from regular git call {}: \"{}\"",
				email,
				count.count
			);

			*trust_map
				.entry(commit_view.commit.hash.to_string())
				.or_insert(true) = (email == &commit_view.committer.email || email == &commit_view.author.email)
				&& count.count >= value_threshold;
		}
	}

	for (commit, trusted) in trust_map.clone() {
		log::debug!(
			"commit_trust_map from regular git call {}: \"{}\"",
			commit,
			trusted
		);
	}

	log::info!("completed {} metric", TRUST_PHASE);

	Ok(Rc::new(CommitTrustOutput {
		commit_counts_in_period: Rc::new(trust_map),
	}))
}
