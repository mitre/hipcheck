// SPDX-License-Identifier: Apache-2.0

use hipcheck_sdk::types::LocalGitRepo;
use jiff::Timestamp;
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

impl From<git2::Commit<'_>> for RawCommit {
	fn from(value: git2::Commit<'_>) -> Self {
		let hash = value.id().to_string();
		let author = &value.author();
		let committer = &value.committer();

		let written_time_sec_since_epoch = jiff::Timestamp::from_second(
			author.when().seconds() + (author.when().offset_minutes() as i64 * 60),
		);
		let commit_time_sec_since_epoch = jiff::Timestamp::from_second(
			committer.when().seconds() + (committer.when().offset_minutes() as i64 * 60),
		);

		RawCommit {
			hash,
			author: author.into(),
			written_on: jiff_timestamp_result_to_string(written_time_sec_since_epoch),
			committer: committer.into(),
			committed_on: jiff_timestamp_result_to_string(commit_time_sec_since_epoch),
		}
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

fn jiff_timestamp_result_to_string(
	timestamp_res: Result<Timestamp, jiff::Error>,
) -> Result<String, String> {
	match timestamp_res {
		Ok(timestamp) => Ok(timestamp.to_string()),
		Err(e) => Err(format!(
			"Error converting commit author time to Timestamp: {}",
			e
		)),
	}
}

impl From<git2::Commit<'_>> for Commit {
	fn from(value: git2::Commit) -> Self {
		let author = &value.author();
		let committer = &value.author();
		let written_on = jiff::Timestamp::from_second(
			author.when().seconds() + (author.when().offset_minutes() as i64 * 60),
		);
		let committed_on = jiff::Timestamp::from_second(
			committer.when().seconds() + (author.when().offset_minutes() as i64 * 60),
		);

		Self {
			hash: value.id().to_string(),
			written_on: jiff_timestamp_result_to_string(written_on),
			committed_on: jiff_timestamp_result_to_string(committed_on),
		}
	}
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

impl From<&git2::Signature<'_>> for Contributor {
	fn from(value: &git2::Signature) -> Self {
		Self {
			name: value.name().unwrap_or_default().to_string(),
			email: value.email().unwrap_or_default().to_string(),
		}
	}
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
	pub additions: i64,
	pub deletions: i64,
	pub patch: String,
}

impl FileDiff {
	pub fn increment_additions(&mut self, amount: i64) {
		self.additions += amount
	}

	pub fn increment_deletions(&mut self, amount: i64) {
		self.additions += amount
	}

	pub fn add_to_patch(&mut self, contents: &[u8]) {
		let contents = String::from_utf8_lossy(contents);
		self.patch.push_str(&contents);
	}
}
