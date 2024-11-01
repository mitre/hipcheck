// SPDX-License-Identifier: Apache-2.0

use hipcheck_sdk::types::LocalGitRepo;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
	fmt::{self, Display, Formatter},
	hash::Hash,
	sync::Arc,
};

/// A locally stored git repo, with optional additional details
/// The details will vary based on the query (e.g. a date, a committer e-mail address, a commit hash)
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DetailedGitRepo {
	/// The local repo
	pub local: LocalGitRepo,

	/// Optional additional information for the query
	pub details: Option<String>,
}

/// Commits as they come directly out of `git log`.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct RawCommit {
	pub hash: String,

	pub author: Contributor,
	pub written_on: Result<String, String>,

	pub committer: Contributor,
	pub committed_on: Result<String, String>,
}

/// Commits as understood in Hipcheck's data model.
/// The `written_on` and `committed_on` datetime fields contain Strings that are created from `jiff:Timestamps`.
/// Because `Timestamp` does not `impl JsonSchema`, we display the datetimes as Strings for passing out of this plugin.
/// Other plugins that expect a `Timestamp`` should parse the provided Strings into `Timestamps` as needed.
#[derive(Debug, Serialize, Clone, PartialEq, Eq, Hash, JsonSchema)]
pub struct Commit {
	pub hash: String,

	pub written_on: Result<String, String>,

	pub committed_on: Result<String, String>,
}

impl Display for Commit {
	fn fmt(&self, f: &mut Formatter) -> fmt::Result {
		write!(f, "{}", self.hash)
	}
}

/// Authors or committers of a commit.
#[derive(
	Debug, PartialEq, Eq, Serialize, Deserialize, Clone, Hash, PartialOrd, Ord, JsonSchema,
)]
pub struct Contributor {
	pub name: String,
	pub email: String,
}

impl Display for Contributor {
	fn fmt(&self, f: &mut Formatter) -> fmt::Result {
		write!(f, "{} <{}>", self.name, self.email)
	}
}

/// "Join struct" for commits and contributors.
#[derive(Debug, PartialEq, Eq, Serialize, Clone)]
pub struct CommitContributor {
	// Index of commit cache
	pub commit_id: usize,
	// Indices of contributor cache
	pub author_id: usize,
	pub committer_id: usize,
}

/// Temporary data structure for looking up the contributors of a commit
#[derive(Debug, Serialize, Clone, PartialEq, Eq, JsonSchema)]
pub struct CommitContributorView {
	pub commit: Commit,
	pub author: Contributor,
	pub committer: Contributor,
}

/// Temporary data structure for looking up the commits associated with a contributor
#[derive(Debug, Serialize, Clone, PartialEq, Eq, JsonSchema)]
pub struct ContributorView {
	pub contributor: Contributor,
	pub commits: Vec<Commit>,
}

/// View into commits and diffs joined together.
#[derive(Debug, Serialize, PartialEq, Eq, JsonSchema)]
pub struct CommitDiff {
	pub commit: Commit,
	pub diff: Diff,
}

impl Display for CommitDiff {
	fn fmt(&self, f: &mut Formatter) -> fmt::Result {
		write!(
			f,
			"{} +{} -{}",
			self.commit,
			self.diff
				.additions
				.map(|n| n.to_string())
				.as_deref()
				.unwrap_or("<unknown>"),
			self.diff
				.deletions
				.map(|n| n.to_string())
				.as_deref()
				.unwrap_or("<unknown>")
		)
	}
}

/// A set of changes in a commit.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct Diff {
	pub additions: Option<i64>,
	pub deletions: Option<i64>,
	pub file_diffs: Vec<FileDiff>,
}

/// A set of changes to a specific file in a commit.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct FileDiff {
	pub file_name: Arc<String>,
	pub additions: Option<i64>,
	pub deletions: Option<i64>,
	pub patch: String,
}
