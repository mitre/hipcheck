// SPDX-License-Identifier: Apache-2.0

use crate::context::Context as _;
use crate::error::Result;
use crate::metric::MetricProvider;
use std::collections::HashMap;
use std::sync::Arc;

pub const TRUST_PHASE: &str = "contributor trust";
pub const PR_TRUST_PHASE: &str = "pull request contributor trust";

#[derive(Debug, Eq, PartialEq)]
pub struct ContributorTrustOutput {
	pub contributor_counts_in_period: Arc<HashMap<String, ContributorTrustResult>>,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct ContributorTrustResult {
	pub count: u32,
	pub repo_trusted: bool,
	pub pr_trusted: bool,
}

//contributor_trust_metric is here mainly for if contributor trust is applied to full repos
//it is not currently used
pub fn contributor_trust_metric(db: &dyn MetricProvider) -> Result<Arc<ContributorTrustOutput>> {
	log::debug!("running {} metric", TRUST_PHASE);

	let month_range = db.contributor_trust_month_count_threshold().to_string();

	let value_threshold = db.contributor_trust_value_threshold() as u32;

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
				pr_trusted: false,
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

//This metric is for pull requests and is active
//A pull request contributor is only trusted if it has x commits >= value_threshold on master
//Does not count merges, merges excluded
pub fn pr_contributor_trust_metric(db: &dyn MetricProvider) -> Result<Arc<ContributorTrustOutput>> {
	log::debug!("running {} metric", PR_TRUST_PHASE);

	let month_range = db.contributor_trust_month_count_threshold().to_string();

	let value_threshold = db.contributor_trust_value_threshold() as u32;

	let commit_from_date = Arc::new(month_range);

	let msg = format!("failed to get commits for {} metric", PR_TRUST_PHASE);

	// Get the commits for the source from a certain date
	let commits = db.commits_from_date(commit_from_date).context(msg)?;

	let mut full_repo_trust_map: HashMap<String, ContributorTrustResult> = HashMap::new();

	for commit in commits.iter() {
		//first we get all commits to the repo and count contributor commits for full repo
		//trusted metrics are really based on the full repo
		// Check if a commit matches the contributor trust rules.
		let msg = format!(
			"failed to get contributor commit view in pr contributor trust for commit in {} metric",
			TRUST_PHASE
		);

		let commit_view = db
			.contributors_for_commit(Arc::clone(commit))
			.context(msg)?;

		let author_email = &commit_view.author.email;
		let committer_email = &commit_view.committer.email;

		let author_entry = full_repo_trust_map
			.entry(author_email.to_string())
			.or_insert(ContributorTrustResult {
				count: 0,
				repo_trusted: false,
				pr_trusted: false,
			});

		author_entry.count += 1;
		author_entry.repo_trusted = author_entry.count >= value_threshold;

		if committer_email != author_email {
			//if committer email is different, count the committer too
			let committer_entry = full_repo_trust_map
				.entry(committer_email.to_string())
				.or_insert(ContributorTrustResult {
					count: 0,
					repo_trusted: false,
					pr_trusted: false,
				});

			committer_entry.count += 1;

			committer_entry.repo_trusted = committer_entry.count >= value_threshold;
		}
	}

	//Now that we have got trust level for all commiters going back (commit_from_date) months, track trust just for the PR
	let pull_request = db
		.single_pull_request_review()
		.context("failed to get single pull request for pr contributor trust metric")?;

	let pr_commits = &pull_request.as_ref().commits;

	let mut pr_trust_map: HashMap<String, ContributorTrustResult> = HashMap::new();

	for pr_commit in pr_commits {
		//get the pr commits and see if any contributors match the full repo
		let commit_view = db
			.get_pr_contributors_for_commit(Arc::clone(pr_commit))
			.context("failed to get commits for pr contributor trust metric")?;
		let author_email = &commit_view.author.email.to_string();
		let committer_email = &commit_view.committer.email.to_string();

		//if author not found in full repo commits, we give it a 0 value and its never trusted
		let author_entry = full_repo_trust_map
			.entry(author_email.to_string())
			.or_insert(ContributorTrustResult {
				count: 0,
				repo_trusted: false,
				pr_trusted: false,
			});
		author_entry.pr_trusted = author_entry.count >= value_threshold;

		pr_trust_map.insert(author_email.to_string(), author_entry.clone());

		if committer_email != author_email {
			//only add or factor in committer if it is different
			//if committer not found in full repo commits, we give it a 0 value and its never trusted
			let committer_entry = full_repo_trust_map
				.entry(committer_email.to_string())
				.or_insert(ContributorTrustResult {
					count: 0,
					repo_trusted: false,
					pr_trusted: false,
				});
			committer_entry.pr_trusted = committer_entry.count >= value_threshold;
			pr_trust_map.insert(committer_email.to_string(), committer_entry.clone());
		}
	}

	for (email, trust_result) in full_repo_trust_map.clone() {
		log::debug!(
			"pr contributor trust metric master trust_map from regular git call email=[{}]: trust_result.count=[{}] trust_result.repo_trusted=[{}]",
			email,
			trust_result.count,
			trust_result.repo_trusted
		);
	}

	for (email, trust_result) in pr_trust_map.clone() {
		log::debug!(
			"pr contributor trust metric pull request contributor trust_map from single pr call email=[{}]: trust_result.count=[{}] trust_result.pr_trusted=[{}]",
			email,
			trust_result.count,
			trust_result.pr_trusted
		);
	}

	log::info!("completed {} metric", PR_TRUST_PHASE);

	Ok(Arc::new(ContributorTrustOutput {
		contributor_counts_in_period: Arc::new(pr_trust_map),
	}))
}
