// SPDX-License-Identifier: Apache-2.0

use crate::{context::Context as _, error::Result, metric::MetricProvider};
use std::{collections::HashMap, sync::Arc};

pub const TRUST_PHASE: &str = "contributor trust";

#[derive(Debug, Eq, PartialEq)]
pub struct ContributorTrustOutput {
	pub contributor_counts_in_period: Arc<HashMap<String, ContributorTrustResult>>,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct ContributorTrustResult {
	pub count: u32,
	pub repo_trusted: bool,
}

//contributor_trust_metric is here mainly for if contributor trust is applied to full repos
//it is not currently used
pub fn contributor_trust_metric(db: &dyn MetricProvider) -> Result<Arc<ContributorTrustOutput>> {
	log::debug!("running {} metric", TRUST_PHASE);

	let month_range = db.contributor_trust_month_count_threshold()?.to_string();

	let value_threshold = db.contributor_trust_value_threshold()? as u32;

	let commit_from_date = Arc::new(month_range);

	let msg = format!("failed to get commits for {} metric", TRUST_PHASE);

	// Get the commits for the source.
	let commits = db.commits_from_date(commit_from_date).context(msg)?;

	let mut trust_map: HashMap<String, ContributorTrustResult> = HashMap::new();

	for commit in commits.iter() {
		// Check if a commit matches the affiliation rules.
		let msg = format!(
			"failed to get contributor commit view for {} metric",
			TRUST_PHASE
		);
		let commit_view = db
			.contributors_for_commit(Arc::clone(commit))
			.context(msg)?;
		let commit_name = &commit_view.author.name;
		let commit_email = &commit_view.author.name;
		log::debug!(
			"full list of commits commit_view.committer.email=[{}] commit_email=[{}] commit_name=[{}]",
			&commit_view.committer.email,
			&commit_email,
			&commit_name
		);

		let entry = trust_map
			.entry(commit_email.to_string())
			.or_insert(ContributorTrustResult {
				count: 0,
				repo_trusted: false,
			});

		entry.count += 1;
		entry.repo_trusted = entry.count >= value_threshold;
	}

	for (email, count) in trust_map.clone() {
		log::debug!(
			"trust_map from regular git call name=[{}]: count.count=[{}] count.repo_trusted=[{}]",
			email,
			count.count,
			count.repo_trusted
		);
	}

	log::info!("completed {} metric", TRUST_PHASE);

	Ok(Arc::new(ContributorTrustOutput {
		contributor_counts_in_period: Arc::new(trust_map),
	}))
}
