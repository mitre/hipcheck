// SPDX-License-Identifier: Apache-2.0

use hipcheck_sdk::types::LocalGitRepo;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
	fmt::{self, Display, Formatter},
	hash::Hash,
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
	pub written_on: Result<jiff::Timestamp, String>,

	pub committer: Contributor,
	pub committed_on: Result<jiff::Timestamp, String>,
}

impl TryFrom<gix::Commit<'_>> for RawCommit {
	type Error = anyhow::Error;

	fn try_from(value: gix::Commit<'_>) -> Result<Self, Self::Error> {
		let commit_author = value.author()?;
		let author = Contributor {
			name: commit_author.name.to_string(),
			email: commit_author.email.to_string(),
		};
		let written_on =
			jiff::Timestamp::from_second(commit_author.time.seconds).map_err(|x| x.to_string());
		let commit_committer = value.committer()?;
		let committer = Contributor {
			name: commit_committer.name.to_string(),
			email: commit_committer.email.to_string(),
		};
		let committed_on =
			jiff::Timestamp::from_second(commit_committer.time.seconds).map_err(|x| x.to_string());

		Ok(Self {
			hash: value.id().to_string(),
			author,
			written_on,
			committer,
			committed_on,
		})
	}
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

impl From<RawCommit> for Commit {
	fn from(value: RawCommit) -> Self {
		Self {
			hash: value.hash,
			written_on: value.written_on.map(|x| x.to_string()),
			committed_on: value.committed_on.map(|x| x.to_string()),
		}
	}
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

/// Data structure for looking up the contributors of a commit
#[derive(Debug, Serialize, Clone, PartialEq, Eq, JsonSchema)]
pub struct CommitContributorView {
	pub commit: Commit,
	pub author: Contributor,
	pub committer: Contributor,
}

impl From<RawCommit> for CommitContributorView {
	fn from(value: RawCommit) -> Self {
		Self {
			commit: Commit {
				hash: value.hash,
				written_on: value.written_on.map(|x| x.to_string()),
				committed_on: value.committed_on.map(|x| x.to_string()),
			},
			author: value.author,
			committer: value.committer,
		}
	}
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

impl CommitDiff {
	pub fn new(commit: Commit, diff: Diff) -> Self {
		Self { commit, diff }
	}
}

impl Display for CommitDiff {
	fn fmt(&self, f: &mut Formatter) -> fmt::Result {
		write!(
			f,
			"{} +{} -{}",
			self.commit, self.diff.additions, self.diff.deletions
		)
	}
}

/// A set of changes in a commit.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct Diff {
	pub additions: i64,
	pub deletions: i64,
	pub file_diffs: Vec<FileDiff>,
}

/// A set of changes to a specific file in a commit.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct FileDiff {
	pub file_name: String,
	pub additions: i64,
	pub deletions: i64,
	pub patch: String,
}

impl FileDiff {
	pub fn new(file_name: String) -> Self {
		Self {
			file_name,
			additions: 0,
			deletions: 0,
			patch: String::new(),
		}
	}

	pub fn increment_additions(&mut self, additions: i64) {
		self.additions += additions
	}

	pub fn increment_deletions(&mut self, deletions: i64) {
		self.deletions += deletions
	}

	pub fn set_patch(&mut self, patch_data: String) {
		self.patch = patch_data;
	}
}
