// SPDX-License-Identifier: Apache-2.0

use chrono::DateTime;
use chrono::FixedOffset;
use serde::Deserialize;
use serde::Serialize;
use serde::{self};
use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::{self};
use std::rc::Rc;

/// Commits as they come directly out of `git log`.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(crate = "self::serde")]
pub struct RawCommit {
	pub hash: String,

	pub author: Contributor,
	pub written_on: Result<DateTime<FixedOffset>, String>,

	pub committer: Contributor,
	pub committed_on: Result<DateTime<FixedOffset>, String>,

	pub signer_name: Option<String>,
	pub signer_key: Option<String>,
}

/// Commits as understood in Hipcheck's data model.
#[derive(Debug, Serialize, Clone, PartialEq, Eq, Hash)]
#[serde(crate = "self::serde")]
pub struct Commit {
	pub hash: String,

	pub written_on: Result<DateTime<FixedOffset>, String>,

	pub committed_on: Result<DateTime<FixedOffset>, String>,
}

impl Display for Commit {
	fn fmt(&self, f: &mut Formatter) -> fmt::Result {
		write!(f, "{}", self.hash)
	}
}

/// Authors or committers of a commit.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Clone, Hash, PartialOrd, Ord)]
#[serde(crate = "self::serde")]
pub struct Contributor {
	pub name: String,
	pub email: String,
}

impl Display for Contributor {
	fn fmt(&self, f: &mut Formatter) -> fmt::Result {
		write!(f, "{} <{}>", self.name, self.email)
	}
}

/// "Joim struct" for commits and contributors.
#[derive(Debug, PartialEq, Eq, Serialize, Clone)]
#[serde(crate = "self::serde")]
pub struct CommitContributor {
	// Index of commit cache
	pub commit_id: usize,
	// Indices of contributor cache
	pub author_id: usize,
	pub committer_id: usize,
}

/// "Join struct" for commits and signature data.
#[derive(Debug, PartialEq, Eq, Serialize, Clone)]
#[serde(crate = "self::serde")]
pub struct CommitSigner {
	// Index of commit cache
	pub commit_id: usize,
	// Index of signer name
	pub signer_name_id: usize,
	// Index of signer key,
	pub signer_key_id: usize,
}

/// Temporary data structure for looking up the contributors of a commit
#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
#[serde(crate = "self::serde")]
pub struct CommitContributorView {
	pub commit: Rc<Commit>,
	pub author: Rc<Contributor>,
	pub committer: Rc<Contributor>,
}

/// Temporary data structure for looking up the commits associated with a contributor
#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
#[serde(crate = "self::serde")]
pub struct ContributorView {
	pub contributor: Rc<Contributor>,
	pub commits: Vec<Rc<Commit>>,
}

/// Temporary data structure for looking up the signer of a commit
#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
#[serde(crate = "self::serde")]
pub struct CommitSignerView {
	pub commit: Rc<Commit>,
	pub signer_name: Option<Rc<String>>,
	pub signer_key: Option<Rc<String>>,
}

/// Temporary data structure for looking up the commits associated with a signer name
#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
#[serde(crate = "self::serde")]
pub struct SignerNameView {
	pub signer_name: Rc<String>,
	pub commits: Vec<Rc<Commit>>,
}

/// Temporary data structure for looking up the commits associated with a signer key
#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
#[serde(crate = "self::serde")]
pub struct SignerKeyView {
	pub signer_key: Rc<String>,
	pub commits: Vec<Rc<Commit>>,
}

/// Temporary data structure for looking up the keys associated with a signer name
#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
#[serde(crate = "self::serde")]
pub struct SignerView {
	pub signer_name: Rc<String>,
	pub signer_keys: Vec<Rc<String>>,
}

/// View into commits and diffs joined together.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(crate = "self::serde")]
pub struct CommitDiff {
	pub commit: Rc<Commit>,
	pub diff: Rc<Diff>,
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
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(crate = "self::serde")]
pub struct Diff {
	pub additions: Option<i64>,
	pub deletions: Option<i64>,
	pub file_diffs: Vec<FileDiff>,
}

/// A set of changes to a specific file in a commit.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(crate = "self::serde")]
pub struct FileDiff {
	pub file_name: Rc<String>,
	pub additions: Option<i64>,
	pub deletions: Option<i64>,
	pub patch: String,
}
