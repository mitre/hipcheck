// SPDX-License-Identifier: Apache-2.0

//! A collection of Salsa query groups for accessing the data used in
//! Hipcheck's analyses.

mod code_quality;
mod dependencies;
mod fuzz;
mod github;
mod module;
mod pr_review;

pub use code_quality::CodeQualityProviderStorage;
pub use dependencies::DependenciesProvider;
pub use dependencies::DependenciesProviderStorage;
pub use fuzz::FuzzProvider;
pub use fuzz::FuzzProviderStorage;
pub use github::GitHubProvider;
pub use github::GitHubProviderStorage;
pub use module::ModuleCommitMap;
pub use module::ModuleProvider;
pub use module::ModuleProviderStorage;
pub use pr_review::PullRequestReviewProvider;
pub use pr_review::PullRequestReviewProviderStorage;
