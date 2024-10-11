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

/// The entropy of a single commit.
#[derive(Debug, Serialize, Clone, JsonSchema, Deserialize)]
pub struct CommitEntropy {
	/// The commit
	pub commit: Commit,
	/// The entropy score
	pub entropy: f64,
}

/// The grapheme frequencies of a single commit.
#[derive(Debug)]
pub struct CommitGraphemeFreq {
	/// The commit.
	pub commit: Commit,
	/// The set of grapheme frequencies.
	pub grapheme_freqs: Vec<GraphemeFreq>,
}

/// The frequency of a single grapheme.
#[derive(Debug)]
pub struct GraphemeFreq {
	/// The grapheme.
	pub grapheme: String,
	/// The frequency.
	pub freq: f64,
}

impl GraphemeFreq {
	pub fn as_view(&self) -> GraphemeFreqView<'_> {
		GraphemeFreqView {
			grapheme: &self.grapheme,
			freq: self.freq,
		}
	}
}

/// A view of a grapheme frequency.
pub struct GraphemeFreqView<'gra> {
	/// The view of the grapheme.
	pub grapheme: &'gra str,
	/// The freq (fine to copy)
	pub freq: f64,
}
