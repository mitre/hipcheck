// SPDX-License-Identifier: Apache-2.0

//! Query group for core Git objects used in Hipcheck's analyses.

mod impls;

use crate::data::git::Commit;
use crate::data::git::CommitContributor;
use crate::data::git::CommitContributorView;
use crate::data::git::CommitDiff;
use crate::data::git::CommitSigner;
use crate::data::git::CommitSignerView;
use crate::data::git::Contributor;
use crate::data::git::ContributorView;
use crate::data::git::Diff;
use crate::data::git::RawCommit;
use crate::data::git::SignerKeyView;
use crate::data::git::SignerNameView;
use crate::data::git::SignerView;
use crate::error::Result;
use crate::source::source::SourceQuery;
use crate::version::VersionQuery;
use chrono::prelude::*;
use std::sync::Arc;

/// Queries about Git objects
#[salsa::query_group(GitProviderStorage)]
pub trait GitProvider: SourceQuery + VersionQuery {
	/// Returns all raw commits extracted from the repository
	#[salsa::invoke(impls::raw_commits)]
	fn raw_commits(&self) -> Result<Arc<Vec<RawCommit>>>;

	/// Returns all raw commits extracted from the repository from a certain date
	#[salsa::invoke(impls::raw_commits_from_date)]
	fn raw_commits_from_date(&self, date: Arc<String>) -> Result<Arc<Vec<RawCommit>>>;

	/// Return the date of the most recent commit
	#[salsa::invoke(impls::last_commit_date)]
	fn last_commit_date(&self) -> Result<DateTime<FixedOffset>>;

	/// Returns all diffs extracted from the repository
	#[salsa::invoke(impls::diffs)]
	fn diffs(&self) -> Result<Arc<Vec<Arc<Diff>>>>;

	/// Returns all commits extracted from the repository
	#[salsa::invoke(impls::commits)]
	fn commits(&self) -> Result<Arc<Vec<Arc<Commit>>>>;

	/// Returns all commits extracted from the repository
	#[salsa::invoke(impls::commits_from_date)]
	fn commits_from_date(&self, date: Arc<String>) -> Result<Arc<Vec<Arc<Commit>>>>;

	/// Returns all contributors to the repository
	#[salsa::invoke(impls::contributors)]
	fn contributors(&self) -> Result<Arc<Vec<Arc<Contributor>>>>;

	/// Returns contributors by commit
	#[salsa::invoke(impls::commit_contributors)]
	fn commit_contributors(&self) -> Result<Arc<Vec<CommitContributor>>>;

	/// Returns all signer names
	#[salsa::invoke(impls::signer_names)]
	fn signer_names(&self) -> Result<Arc<Vec<Option<Arc<String>>>>>;

	/// Returns all signer keys
	#[salsa::invoke(impls::signer_keys)]
	fn signer_keys(&self) -> Result<Arc<Vec<Option<Arc<String>>>>>;

	/// Returns all commit signers
	#[salsa::invoke(impls::commit_signers)]
	fn commit_signers(&self) -> Result<Arc<Vec<CommitSigner>>>;

	/// Returns all commit-diff pairs
	#[salsa::invoke(impls::commit_diffs)]
	fn commit_diffs(&self) -> Result<Arc<Vec<CommitDiff>>>;

	/// Returns the commits associated with a given contributor
	#[salsa::invoke(impls::commits_for_contributor)]
	fn commits_for_contributor(&self, contributor: Arc<Contributor>) -> Result<ContributorView>;

	/// Returns the contributor view for a given commit
	#[salsa::invoke(impls::contributors_for_commit)]
	fn contributors_for_commit(&self, commit: Arc<Commit>) -> Result<CommitContributorView>;

	/// Returns the commits associated with a given signer name
	#[salsa::invoke(impls::commits_for_signer_name)]
	fn commits_for_signer_name(&self, signer_name: Arc<String>) -> Result<SignerNameView>;

	/// Returns the commits associated with a given signer key
	#[salsa::invoke(impls::commits_for_signer_key)]
	fn commits_for_signer_key(&self, signer_key: Arc<String>) -> Result<SignerKeyView>;

	/// Returns the signer name and key, if any, associated with a
	/// given commit
	#[salsa::invoke(impls::signer_for_commit)]
	fn signer_for_commit(&self, commit: Arc<Commit>) -> Result<CommitSignerView>;

	/// Returns the signer view for a given signer name
	#[salsa::invoke(impls::signer_for_name)]
	fn signer_for_name(&self, signer_name: Arc<String>) -> Result<SignerView>;

	/// Returns the signer view for a given signer key
	#[salsa::invoke(impls::signer_for_key)]
	fn signer_for_key(&self, signer_key: Arc<String>) -> Result<SignerView>;

	/// Returns shorter form of a given git hash
	#[salsa::invoke(impls::get_short_hash)]
	fn get_short_hash(&self, long_hash: Arc<String>) -> Result<String>;
}
