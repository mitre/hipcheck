// SPDX-License-Identifier: Apache-2.0

use crate::{context::Context as _, data::PullRequest, error::Result, metric::MetricProvider};
use serde::Serialize;
use std::sync::Arc;

#[derive(Debug, Eq, PartialEq, Serialize)]
pub struct ReviewOutput {
	pub pull_reviews: Vec<PullReview>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct PullReview {
	pub pull_request: Arc<PullRequest>,
	pub has_review: bool,
}

pub fn review_metric(db: &dyn MetricProvider) -> Result<Arc<ReviewOutput>> {
	log::debug!("running review metric");

	let pull_requests = db
		.pull_request_reviews()
		.context("failed to get pull request reviews")?;

	log::trace!("got pull requests [requests='{:#?}']", pull_requests);

	let mut pull_reviews = Vec::with_capacity(pull_requests.len());

	for pull_request in pull_requests.as_ref() {
		let has_review = pull_request.reviews > 0;
		pull_reviews.push(PullReview {
			pull_request: Arc::clone(pull_request),
			has_review,
		});
	}

	log::info!("completed review metric");

	Ok(Arc::new(ReviewOutput { pull_reviews }))
}
