// SPDX-License-Identifier: Apache-2.0

//! Logic for interacting with the GitHub GraphQL API.

mod custom_scalars;
pub mod reviews;
pub mod user_orgs;

/// The URL of the GitHub GraphQL API.
pub const GH_API_V4: &str = "https://api.github.com/graphql";
