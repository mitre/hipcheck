// SPDX-License-Identifier: Apache-2.0

pub mod binary_detector;
pub mod commit_trust;
pub mod contributor_trust;
pub mod linguist;

use crate::{
	config::{AttacksConfigQuery, CommitConfigQuery},
	data::{git::GitProvider, DependenciesProvider, FuzzProvider, PullRequestReviewProvider},
	error::Result,
	metric::{
		binary_detector::BinaryFile, commit_trust::CommitTrustOutput,
		contributor_trust::ContributorTrustOutput, linguist::Linguist,
	},
};
use std::sync::Arc;

/// Queries about metrics
#[salsa::query_group(MetricProviderStorage)]
pub trait MetricProvider:
	AttacksConfigQuery
	+ BinaryFile
	+ CommitConfigQuery
	+ DependenciesProvider
	+ GitProvider
	+ Linguist
	+ FuzzProvider
	+ PullRequestReviewProvider
{	
	/// Returns result of contributor trust metric
	#[salsa::invoke(commit_trust::commit_trust_metric)]
	fn commit_trust_metric(&self) -> Result<Arc<CommitTrustOutput>>;

	/// Returns result of contributor trust metric
	#[salsa::invoke(contributor_trust::contributor_trust_metric)]
	fn contributor_trust_metric(&self) -> Result<Arc<ContributorTrustOutput>>;
}