// SPDX-License-Identifier: Apache-2.0

pub mod activity;
pub mod affiliation;
pub mod binary;
pub mod binary_detector;
pub mod churn;
pub mod commit_trust;
pub mod contributor_trust;
pub mod entropy;
pub mod fuzz;
pub mod identity;
pub mod linguist;
mod math;
pub mod module;
pub mod review;
pub mod typo;

use crate::config::AttacksConfigQuery;
use crate::config::CommitConfigQuery;
use crate::data::git::GitProvider;
use crate::data::DependenciesProvider;
use crate::data::FuzzProvider;
use crate::data::ModuleProvider;
use crate::data::PullRequestReviewProvider;
use crate::error::Result;
use crate::metric::activity::ActivityOutput;
use crate::metric::affiliation::AffiliationOutput;
use crate::metric::binary::BinaryOutput;
use crate::metric::binary_detector::BinaryFile;
use crate::metric::churn::ChurnOutput;
use crate::metric::commit_trust::CommitTrustOutput;
use crate::metric::contributor_trust::ContributorTrustOutput;
use crate::metric::entropy::EntropyOutput;
use crate::metric::fuzz::FuzzOutput;
use crate::metric::identity::IdentityOutput;
use crate::metric::linguist::Linguist;
use crate::metric::module::ModuleOutput;
use crate::metric::review::ReviewOutput;
use crate::metric::typo::TypoOutput;
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
	+ ModuleProvider
	+ FuzzProvider
	+ PullRequestReviewProvider
{
	/// Returns result of activity metric
	#[salsa::invoke(activity::activity_metric)]
	fn activity_metric(&self) -> Result<Arc<ActivityOutput>>;

	/// Returns result of affiliation metric
	#[salsa::invoke(affiliation::affiliation_metric)]
	fn affiliation_metric(&self) -> Result<Arc<AffiliationOutput>>;

	/// Returns result of binary metric
	#[salsa::invoke(binary::binary_metric)]
	fn binary_metric(&self) -> Result<Arc<BinaryOutput>>;

	/// Returns result of churn metric
	#[salsa::invoke(churn::churn_metric)]
	fn churn_metric(&self) -> Result<Arc<ChurnOutput>>;

	/// Returns result of contributor trust metric
	#[salsa::invoke(commit_trust::commit_trust_metric)]
	fn commit_trust_metric(&self) -> Result<Arc<CommitTrustOutput>>;

	/// Returns result of contributor trust metric
	#[salsa::invoke(contributor_trust::contributor_trust_metric)]
	fn contributor_trust_metric(&self) -> Result<Arc<ContributorTrustOutput>>;

	/// Returns result of entropy metric
	#[salsa::invoke(entropy::entropy_metric)]
	fn entropy_metric(&self) -> Result<Arc<EntropyOutput>>;

	/// Returns result of identity metric
	#[salsa::invoke(identity::identity_metric)]
	fn identity_metric(&self) -> Result<Arc<IdentityOutput>>;

	/// Returns result of module analysis.
	#[salsa::invoke(module::module_analysis)]
	fn module_analysis(&self) -> Result<Arc<ModuleOutput>>;

	/// Returns result of fuzz metric
	#[salsa::invoke(fuzz::fuzz_metric)]
	fn fuzz_metric(&self) -> Result<Arc<FuzzOutput>>;

	/// Returns result of review metric
	#[salsa::invoke(review::review_metric)]
	fn review_metric(&self) -> Result<Arc<ReviewOutput>>;

	/// Returns result of typo metric
	#[salsa::invoke(typo::typo_metric)]
	fn typo_metric(&self) -> Result<Arc<TypoOutput>>;
}
