// SPDX-License-Identifier: Apache-2.0

use serde::Deserialize;

/// Repository collaborator data pulled from the GitHub API.
#[derive(Debug, Default, Deserialize, Clone)]
pub struct GitHubCollaborator {
	#[allow(unused)]
	/// The user's username.
	pub login: String,
	/// The user's public email address.
	pub email: String,
	#[allow(unused)]
	/// The user's employer, as pulled from their GitHub profile.
	pub profile_employer: Option<String>,
	/// The GitHub organizations the user belongs to.
	pub github_orgs: Vec<GitHubOrg>,
}

/// A single organization on GitHub.
#[derive(Debug, Deserialize, Clone)]
pub struct GitHubOrg {
	#[allow(unused)]
	/// The display name of the organization, if present.
	pub name: Option<String>,
	#[allow(unused)]
	/// The username of the organization.
	pub login: String,
	/// The organization's email, if present.
	pub email: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct GitHubRepoContributor {
	/// The user's email.
	pub email: Option<String>,

	/// The user's username (GitHub calls them "logins").
	pub login: Option<String>,
}
