// SPDX-License-Identifier: Apache-2.0

//! Derived query implementations for the `GitProvider` query group.

use super::GitProvider;
use crate::context::Context;
use crate::data::git::get_commits;
use crate::data::git::get_commits_from_date;
use crate::data::git::get_diffs;
use crate::data::git::Commit;
use crate::data::git::CommitContributor;
use crate::data::git::CommitContributorView;
use crate::data::git::CommitDiff;
use crate::data::git::CommitSigner;
use crate::data::git::CommitSignerView;
use crate::data::git::Contributor;
use crate::data::git::ContributorView;
use crate::data::git::Diff;
use crate::data::git::GitCommand;
use crate::data::git::RawCommit;
use crate::data::git::SignerKeyView;
use crate::data::git::SignerNameView;
use crate::data::git::SignerView;
use crate::error::Error;
use crate::error::Result;
use chrono::prelude::*;
use std::sync::Arc;

pub(crate) fn raw_commits(db: &dyn GitProvider) -> Result<Arc<Vec<RawCommit>>> {
	get_commits(db.local().as_ref()).map(Arc::new)
}

pub(crate) fn raw_commits_from_date(
	db: &dyn GitProvider,
	date: Arc<String>,
) -> Result<Arc<Vec<RawCommit>>> {
	get_commits_from_date(db.local().as_ref(), date.as_str()).map(Arc::new)
}

pub(crate) fn last_commit_date(db: &dyn GitProvider) -> Result<DateTime<FixedOffset>> {
	let commits = db.raw_commits().context("failed to get raw commits")?;

	let first = commits.first().ok_or_else(|| Error::msg("no commits"))?;

	first
		.written_on
		.as_ref()
		.map(|dt| *dt)
		.map_err(|e| Error::msg(e.clone()))
}

pub(crate) fn diffs(db: &dyn GitProvider) -> Result<Arc<Vec<Arc<Diff>>>> {
	let diffs = get_diffs(db.local().as_ref())?;
	let diffs = diffs.into_iter().map(Arc::new).collect();

	Ok(Arc::new(diffs))
}

//accepts a date parameter in form at 2021-09-10 to filter commits from git
pub(crate) fn commits_from_date(
	db: &dyn GitProvider,
	date: Arc<String>,
) -> Result<Arc<Vec<Arc<Commit>>>> {
	let commits = db
		.raw_commits_from_date(Arc::new(date.to_string()))
		.context("failed to get raw commits from date")?
		.iter()
		.map(|raw| {
			Arc::new(Commit {
				hash: raw.hash.to_owned(),
				written_on: raw.written_on.to_owned(),
				committed_on: raw.committed_on.to_owned(),
			})
		})
		.collect();

	Ok(Arc::new(commits))
}

pub(crate) fn commits(db: &dyn GitProvider) -> Result<Arc<Vec<Arc<Commit>>>> {
	let commits = db
		.raw_commits()
		.context("failed to get raw commits")?
		.iter()
		.map(|raw| {
			Arc::new(Commit {
				hash: raw.hash.to_owned(),
				written_on: raw.written_on.to_owned(),
				committed_on: raw.committed_on.to_owned(),
			})
		})
		.collect();

	Ok(Arc::new(commits))
}

pub(crate) fn contributors(db: &dyn GitProvider) -> Result<Arc<Vec<Arc<Contributor>>>> {
	let mut contributors: Vec<_> = db
		.raw_commits()
		.context("failed to get raw commits")?
		.iter()
		.flat_map(|raw| {
			[
				Arc::new(raw.author.to_owned()),
				Arc::new(raw.committer.to_owned()),
			]
		})
		.collect();

	contributors.sort();
	contributors.dedup();

	Ok(Arc::new(contributors))
}

pub(crate) fn commit_contributors(db: &dyn GitProvider) -> Result<Arc<Vec<CommitContributor>>> {
	let contributors = db.contributors().context("failed to get contributors")?;

	let commit_contributors = db
		.raw_commits()
		.context("failed to get raw commits")?
		.iter()
		.enumerate()
		.map(|(commit_id, raw)| {
			// SAFETY: These `position` calls are guaranteed to return `Some`
			// given how `contributors` is constructed from `db.raw_commits()`
			let author_id = contributors
				.iter()
				.position(|c| c.as_ref() == &raw.author)
				.unwrap();
			let committer_id = contributors
				.iter()
				.position(|c| c.as_ref() == &raw.committer)
				.unwrap();

			CommitContributor {
				commit_id,
				author_id,
				committer_id,
			}
		})
		.collect();

	Ok(Arc::new(commit_contributors))
}

pub(crate) fn signer_names(db: &dyn GitProvider) -> Result<Arc<Vec<Option<Arc<String>>>>> {
	let mut signer_names: Vec<_> = db
		.raw_commits()
		.context("failed to get raw commits")?
		.iter()
		.map(|raw| raw.signer_name.to_owned().map(Arc::new))
		.collect();

	signer_names.sort();
	signer_names.dedup();

	Ok(Arc::new(signer_names))
}

pub(crate) fn signer_keys(db: &dyn GitProvider) -> Result<Arc<Vec<Option<Arc<String>>>>> {
	let mut signer_keys: Vec<_> = db
		.raw_commits()
		.context("failed to get raw commits")?
		.iter()
		.map(|raw| raw.signer_key.to_owned().map(Arc::new))
		.collect();

	signer_keys.sort();
	signer_keys.dedup();

	Ok(Arc::new(signer_keys))
}

pub(crate) fn commit_signers(db: &dyn GitProvider) -> Result<Arc<Vec<CommitSigner>>> {
	let signer_names = db.signer_names().context("failed to get signer names")?;
	let signer_keys = db.signer_keys().context("failed to get signer keys")?;

	let commit_signers = db
		.raw_commits()
		.context("failed to get raw commits")?
		.iter()
		.enumerate()
		.map(|(commit_id, raw)| {
			// SAFETY: These `position` calls are guaranteed to return `Some`
			// given how `signer_names` and `signer_keys` are constructed from
			// `db.raw_commits()`
			let signer_name_id = signer_names
				.iter()
				.position(|n| n.as_deref() == raw.signer_name.as_ref())
				.unwrap();
			let signer_key_id = signer_keys
				.iter()
				.position(|k| k.as_deref() == raw.signer_key.as_ref())
				.unwrap();

			CommitSigner {
				commit_id,
				signer_name_id,
				signer_key_id,
			}
		})
		.collect();

	Ok(Arc::new(commit_signers))
}

pub(crate) fn commit_diffs(db: &dyn GitProvider) -> Result<Arc<Vec<CommitDiff>>> {
	let commits = db.commits().context("failed to get commits")?;
	let diffs = db.diffs().context("failed to get diffs")?;

	let commit_diffs = Iterator::zip(commits.iter(), diffs.iter())
		.map(|(commit, diff)| CommitDiff {
			commit: Arc::clone(commit),
			diff: Arc::clone(diff),
		})
		.collect();

	Ok(Arc::new(commit_diffs))
}

pub(crate) fn commits_for_contributor(
	db: &dyn GitProvider,
	contributor: Arc<Contributor>,
) -> Result<ContributorView> {
	let all_commits = db.commits().context("failed to get commits")?;
	let contributors = db.contributors().context("failed to get contributors")?;
	let commit_contributors = db
		.commit_contributors()
		.context("failed to get join table")?;

	// Get the index of the contributor
	let contributor_id = contributors
		.iter()
		.position(|c| c == &contributor)
		.ok_or_else(|| Error::msg("failed to find contributor"))?;

	// Find commits that have that contributor
	let commits = commit_contributors
		.iter()
		.filter_map(|com_con| {
			if com_con.author_id == contributor_id || com_con.committer_id == contributor_id {
				// SAFETY: This index is guaranteed to be valid in
				// `all_commits` because of how it and `commit_contributors`
				// are constructed from `db.raw_commits()`
				Some(Arc::clone(&all_commits[com_con.commit_id]))
			} else {
				None
			}
		})
		.collect();

	Ok(ContributorView {
		contributor,
		commits,
	})
}

pub(crate) fn contributors_for_commit(
	db: &dyn GitProvider,
	commit: Arc<Commit>,
) -> Result<CommitContributorView> {
	let commits = db.commits().context("failed to get commits")?;
	let contributors = db.contributors().context("failed to get contributors")?;
	let commit_contributors = db
		.commit_contributors()
		.context("failed to get join table")?;

	// Get the index of the commit
	let commit_id = commits
		.iter()
		.position(|c| c.hash == commit.hash)
		.ok_or_else(|| Error::msg("failed to find commit"))?;

	// Find the author and committer for that commit
	commit_contributors
		.iter()
		.find(|com_con| com_con.commit_id == commit_id)
		.map(|com_con| {
			// SAFETY: These indices are guaranteed to be valid in
			// `contributors` because of how `commit_contributors` is
			// constructed from it.
			let author = Arc::clone(&contributors[com_con.author_id]);
			let committer = Arc::clone(&contributors[com_con.committer_id]);

			CommitContributorView {
				commit,
				author,
				committer,
			}
		})
		.ok_or_else(|| Error::msg("failed to find contributor info"))
}

pub(crate) fn commits_for_signer_name(
	db: &dyn GitProvider,
	signer_name: Arc<String>,
) -> Result<SignerNameView> {
	let all_commits = db.commits().context("failed to get commits")?;
	let signer_names = db.signer_names().context("failed to get signer names")?;
	let commit_signers = db.commit_signers().context("failed to get join table")?;

	// Get the index of the signer name
	let signer_name_id = signer_names
		.iter()
		.position(|n| n.as_ref() == Some(&signer_name))
		.ok_or_else(|| Error::msg("failed to find signer name"))?;

	// Find commits that have that signer name
	let commits = commit_signers
		.iter()
		.filter_map(|com_sign| {
			if com_sign.signer_name_id == signer_name_id {
				// SAFETY: This index is guaranteed to be valid in
				// `all_commits` because of how it and `commit_signers`
				// are constructed from `db.raw_commits()`.
				Some(Arc::clone(&all_commits[com_sign.commit_id]))
			} else {
				None
			}
		})
		.collect();

	Ok(SignerNameView {
		signer_name,
		commits,
	})
}

pub(crate) fn commits_for_signer_key(
	db: &dyn GitProvider,
	signer_key: Arc<String>,
) -> Result<SignerKeyView> {
	let all_commits = db.commits().context("failed to get commits")?;
	let signer_keys = db.signer_keys().context("failed to get signer keys")?;
	let commit_signers = db.commit_signers().context("failed to get join table")?;

	// Get the index of the signer name
	let signer_key_id = signer_keys
		.iter()
		.position(|k| k.as_ref() == Some(&signer_key))
		.ok_or_else(|| Error::msg("failed to find signer key"))?;

	// Find commits that have that signer key
	let commits = commit_signers
		.iter()
		.filter_map(|com_sign| {
			if com_sign.signer_key_id == signer_key_id {
				// SAFETY: This index is guaranteed to be valid in
				// `all_commits` because of how it and `commit_signers`
				// are constructed from `db.raw_commits()`.
				Some(Arc::clone(&all_commits[com_sign.commit_id]))
			} else {
				None
			}
		})
		.collect();

	Ok(SignerKeyView {
		signer_key,
		commits,
	})
}

pub(crate) fn signer_for_commit(
	db: &dyn GitProvider,
	commit: Arc<Commit>,
) -> Result<CommitSignerView> {
	let commits = db.commits().context("failed to get commits")?;
	let signer_names = db.signer_names().context("failed to get signer names")?;
	let signer_keys = db.signer_keys().context("failed to get signer keys")?;
	let commit_signers = db.commit_signers().context("failed to get join table")?;

	// Get the index of the commit
	let commit_id = commits
		.iter()
		.position(|c| c.hash == commit.hash)
		.ok_or_else(|| Error::msg("failed to find commit"))?;

	// Find the signer name and keys for that commit
	commit_signers
		.iter()
		.find(|com_sign| com_sign.commit_id == commit_id)
		.map(|com_sign| {
			// SAFETY: These indices are guaranteed to be valid in
			// `signer_names` and `signer_keys` because of how these
			// and `commit_signers` are constructed
			let signer_name = signer_names[com_sign.signer_name_id].clone();
			let signer_key = signer_keys[com_sign.signer_key_id].clone();

			CommitSignerView {
				commit,
				signer_name,
				signer_key,
			}
		})
		.ok_or_else(|| Error::msg("failed to find contributor info"))
}

pub(crate) fn signer_for_name(
	db: &dyn GitProvider,
	signer_name: Arc<String>,
) -> Result<SignerView> {
	let signer_names = db.signer_names().context("failed to get signer names")?;
	let all_signer_keys = db.signer_keys().context("failed to get signer keys")?;
	let commit_signers = db.commit_signers().context("failed to get join table")?;

	// Get the index of the signer name
	let signer_name_id = signer_names
		.iter()
		.position(|n| n.as_ref() == Some(&signer_name))
		.ok_or_else(|| Error::msg("failed to find signer name"))?;

	// Find keys that have that signer name
	// Skips cases where there is a signer name but no signer key
	let signer_keys = commit_signers
		.iter()
		.filter_map(|com_sign| {
			// SAFETY: This index is guaranteed to be valid in
			// `all_signer_keys` because of how it and
			// `commit_signers` are constructed
			if com_sign.signer_name_id == signer_name_id {
				all_signer_keys[com_sign.signer_key_id].clone()
			} else {
				None
			}
		})
		.collect();

	Ok(SignerView {
		signer_name,
		signer_keys,
	})
}

pub(crate) fn signer_for_key(db: &dyn GitProvider, signer_key: Arc<String>) -> Result<SignerView> {
	let signer_names = db.signer_names().context("failed to get signer names")?;
	let all_signer_keys = db.signer_keys().context("failed to get signer keys")?;
	let commit_signers = db.commit_signers().context("failed to get join table")?;

	// Get the index of the signer key
	let signer_key_id = all_signer_keys
		.iter()
		.position(|k| k.as_ref() == Some(&signer_key))
		.ok_or_else(|| Error::msg("failed to find signer key"))?;

	// Get the index of the signer name
	let signer_name_id = commit_signers
		.iter()
		.find(|com_sign| com_sign.signer_key_id == signer_key_id)
		.map(|com_sign| com_sign.signer_name_id)
		.ok_or_else(|| Error::msg("failed to find join table entry"))?;

	// Get the signer name itself
	// SAFETY: This index is guaranteed to be valid in `signer_names`
	// because of how `commit_signers` is constructed
	let signer_name = signer_names[signer_name_id]
		.clone()
		.ok_or_else(|| Error::msg("failed to find signer name"))?;

	// Find keys that have that signer name
	db.signer_for_name(signer_name)
}

pub(crate) fn get_short_hash(
	db: &dyn GitProvider,
	long_hash: impl AsRef<String>,
) -> Result<String> {
	let repo = db.local();
	let repo_path = repo.as_path();
	let output = GitCommand::for_repo(repo_path, ["rev-parse", "--short", long_hash.as_ref()])?
		.output()
		.context("git rev-parse command failed")?;

	Ok(output)
}
