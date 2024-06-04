// SPDX-License-Identifier: Apache-2.0

use std::rc::Rc;

use crate::config::AttacksConfigQuery;
use crate::config::CommitConfigQuery;
use crate::data::git::GitProvider;
use crate::data::DependenciesProvider;
use crate::data::FuzzProvider;
use crate::data::ModuleProvider;
use crate::data::PullRequestReviewProvider;
use crate::error::Result;
use crate::metric::activity::{self, ActivityOutput};
use crate::metric::affiliation::{self, AffiliationOutput};
use crate::metric::binary::{self, BinaryOutput};
use crate::metric::binary_detector::BinaryFile;
use crate::metric::churn::{self, ChurnOutput};
use crate::metric::commit_trust::{self, CommitTrustOutput};
use crate::metric::contributor_trust::{self, ContributorTrustOutput};
use crate::metric::entropy::{self, EntropyOutput};
use crate::metric::fuzz::{self, FuzzOutput};
use crate::metric::identity::{self, IdentityOutput};
use crate::metric::linguist::Linguist;
use crate::metric::module::{self, ModuleOutput};
use crate::metric::module_contributors::{self, ModuleContributorsOutput};
use crate::metric::review::{self, ReviewOutput};
use crate::metric::typo::{self, TypoOutput};

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
	fn activity_metric(&self) -> Result<Rc<ActivityOutput>>;

	/// Returns result of affiliation metric
	#[salsa::invoke(affiliation::affiliation_metric)]
	fn affiliation_metric(&self) -> Result<Rc<AffiliationOutput>>;

	/// Returns result of binary metric
	#[salsa::invoke(binary::binary_metric)]
	fn binary_metric(&self) -> Result<Rc<BinaryOutput>>;

	/// Returns result of churn metric
	#[salsa::invoke(churn::churn_metric)]
	fn churn_metric(&self) -> Result<Rc<ChurnOutput>>;

	/// Returns result of contributor trust metric
	#[salsa::invoke(commit_trust::commit_trust_metric)]
	fn commit_trust_metric(&self) -> Result<Rc<CommitTrustOutput>>;

	/// Returns result of contributor trust metric
	#[salsa::invoke(contributor_trust::contributor_trust_metric)]
	fn contributor_trust_metric(&self) -> Result<Rc<ContributorTrustOutput>>;

	/// Returns result of entropy metric
	#[salsa::invoke(entropy::entropy_metric)]
	fn entropy_metric(&self) -> Result<Rc<EntropyOutput>>;

	/// Returns result of identity metric
	#[salsa::invoke(identity::identity_metric)]
	fn identity_metric(&self) -> Result<Rc<IdentityOutput>>;

	/// Returns result of module analysis.
	#[salsa::invoke(module::module_analysis)]
	fn module_analysis(&self) -> Result<Rc<ModuleOutput>>;

	/// Returns result of fuzz metric
	#[salsa::invoke(fuzz::fuzz_metric)]
	fn fuzz_metric(&self) -> Result<Rc<FuzzOutput>>;

	/// Returns result of review metric
	#[salsa::invoke(review::review_metric)]
	fn review_metric(&self) -> Result<Rc<ReviewOutput>>;

	/// Returns result of typo metric
	#[salsa::invoke(typo::typo_metric)]
	fn typo_metric(&self) -> Result<Rc<TypoOutput>>;

	/// Returns result of pull request affiliation metric
	#[salsa::invoke(affiliation::pr_affiliation_metric)]
	fn pr_affiliation_metric(&self) -> Result<Rc<AffiliationOutput>>;

	/// Returns result of pull request contributor trust metric
	#[salsa::invoke(contributor_trust::pr_contributor_trust_metric)]
	fn pr_contributor_trust_metric(&self) -> Result<Rc<ContributorTrustOutput>>;

	/// Returns result of pull request module contributors metric
	#[salsa::invoke(module_contributors::pr_module_contributors_metric)]
	fn pr_module_contributors_metric(&self) -> Result<Rc<ModuleContributorsOutput>>;
}
