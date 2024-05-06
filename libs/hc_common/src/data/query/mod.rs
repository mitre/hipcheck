// SPDX-License-Identifier: Apache-2.0

//! A collection of Salsa query groups for accessing the data used in
//! Hipcheck's analyses.

mod code_quality;
mod dependencies;
mod fuzz;
mod github;
mod module;
mod pr_review;

pub use code_quality::{CodeQualityProvider, CodeQualityProviderStorage};
pub use dependencies::{DependenciesProvider, DependenciesProviderStorage};
pub use fuzz::{FuzzProvider, FuzzProviderStorage};
pub use github::{GitHubProvider, GitHubProviderStorage};
pub use module::{ModuleCommitMap, ModuleProvider, ModuleProviderStorage};
pub use pr_review::{PullRequestReviewProvider, PullRequestReviewProviderStorage};
