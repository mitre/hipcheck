// SPDX-License-Identifier: Apache-2.0

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::result::Result;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash, JsonSchema)]
pub struct Commit {
	pub hash: String,
	pub written_on: Result<String, String>,
	pub committed_on: Result<String, String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq, JsonSchema, Deserialize)]
pub struct FileDiff {
	pub file_name: String,
	pub additions: Option<i64>,
	pub deletions: Option<i64>,
	pub patch: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq, JsonSchema, Deserialize)]
pub struct Diff {
	pub additions: Option<i64>,
	pub deletions: Option<i64>,
	pub file_diffs: Vec<FileDiff>,
}

#[derive(Debug, Serialize, PartialEq, Eq, JsonSchema, Deserialize)]
pub struct CommitDiff {
	pub commit: Commit,
	pub diff: Diff,
}

#[derive(Debug, Clone, JsonSchema, Serialize)]
pub struct CommitChurnFreq {
	/// The commit
	pub commit: Commit,
	/// The churn score
	pub churn: f64,
}

#[derive(Debug)]
pub struct CommitChurn {
	pub commit: Commit,
	pub files_changed: i64,
	pub lines_changed: i64,
}
