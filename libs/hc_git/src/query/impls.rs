// SPDX-License-Identifier: Apache-2.0

//! Derived query implementations for the `GitProvider` query group.

use super::GitProvider;
use crate::{
	get_commits, get_commits_from_date, get_diffs, Commit, CommitContributor,
	CommitContributorView, CommitDiff, CommitSigner, CommitSignerView, Contributor,
	ContributorView, Diff, GitCommand, RawCommit, SignerKeyView, SignerNameView, SignerView,
};
use hc_common::{
	chrono::prelude::*,
	context::Context,
	error::{Error, Result},
};
use std::rc::Rc;

pub(crate) fn raw_commits(db: &dyn GitProvider) -> Result<Rc<Vec<RawCommit>>> {
	get_commits(db.local().as_ref()).map(Rc::new)
}

pub(crate) fn raw_commits_from_date(
	db: &dyn GitProvider,
	date: Rc<String>,
) -> Result<Rc<Vec<RawCommit>>> {
	get_commits_from_date(db.local().as_ref(), date.as_str()).map(Rc::new)
}

pub(crate) fn last_commit_date(db: &dyn GitProvider) -> Result<Date<FixedOffset>> {
	let commits = db.raw_commits().context("failed to get raw commits")?;

	let first = commits.first().ok_or_else(|| Error::msg("no commits"))?;

	first
		.written_on
		.as_ref()
		.map(|date| date.date())
		.map_err(|e| Error::msg(e.clone()))
}

pub(crate) fn diffs(db: &dyn GitProvider) -> Result<Rc<Vec<Rc<Diff>>>> {
	let diffs = get_diffs(db.local().as_ref())?;
	let diffs = diffs.into_iter().map(Rc::new).collect();

	Ok(Rc::new(diffs))
}

//accepts a date parameter in form at 2021-09-10 to filter commits from git
pub(crate) fn commits_from_date(
	db: &dyn GitProvider,
	date: Rc<String>,
) -> Result<Rc<Vec<Rc<Commit>>>> {
	let commits = db
		.raw_commits_from_date(Rc::new(date.to_string()))
		.context("failed to get raw commits from date")?
		.iter()
		.map(|raw| {
			Rc::new(Commit {
				hash: raw.hash.to_owned(),
				written_on: raw.written_on.to_owned(),
				committed_on: raw.committed_on.to_owned(),
			})
		})
		.collect();

	Ok(Rc::new(commits))
}

pub(crate) fn commits(db: &dyn GitProvider) -> Result<Rc<Vec<Rc<Commit>>>> {
	let commits = db
		.raw_commits()
		.context("failed to get raw commits")?
		.iter()
		.map(|raw| {
			Rc::new(Commit {
				hash: raw.hash.to_owned(),
				written_on: raw.written_on.to_owned(),
				committed_on: raw.committed_on.to_owned(),
			})
		})
		.collect();

	Ok(Rc::new(commits))
}

pub(crate) fn contributors(db: &dyn GitProvider) -> Result<Rc<Vec<Rc<Contributor>>>> {
	let mut contributors: Vec<_> = db
		.raw_commits()
		.context("failed to get raw commits")?
		.iter()
		.flat_map(|raw| {
			[
				Rc::new(raw.author.to_owned()),
				Rc::new(raw.committer.to_owned()),
			]
		})
		.collect();

	contributors.sort();
	contributors.dedup();

	Ok(Rc::new(contributors))
}

pub(crate) fn commit_contributors(db: &dyn GitProvider) -> Result<Rc<Vec<CommitContributor>>> {
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

	Ok(Rc::new(commit_contributors))
}

pub(crate) fn signer_names(db: &dyn GitProvider) -> Result<Rc<Vec<Option<Rc<String>>>>> {
	let mut signer_names: Vec<_> = db
		.raw_commits()
		.context("failed to get raw commits")?
		.iter()
		.map(|raw| raw.signer_name.to_owned().map(Rc::new))
		.collect();

	signer_names.sort();
	signer_names.dedup();

	Ok(Rc::new(signer_names))
}

pub(crate) fn signer_keys(db: &dyn GitProvider) -> Result<Rc<Vec<Option<Rc<String>>>>> {
	let mut signer_keys: Vec<_> = db
		.raw_commits()
		.context("failed to get raw commits")?
		.iter()
		.map(|raw| raw.signer_key.to_owned().map(Rc::new))
		.collect();

	signer_keys.sort();
	signer_keys.dedup();

	Ok(Rc::new(signer_keys))
}

pub(crate) fn commit_signers(db: &dyn GitProvider) -> Result<Rc<Vec<CommitSigner>>> {
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

	Ok(Rc::new(commit_signers))
}

pub(crate) fn commit_diffs(db: &dyn GitProvider) -> Result<Rc<Vec<CommitDiff>>> {
	let commits = db.commits().context("failed to get commits")?;
	let diffs = db.diffs().context("failed to get diffs")?;

	let commit_diffs = Iterator::zip(commits.iter(), diffs.iter())
		.map(|(commit, diff)| CommitDiff {
			commit: Rc::clone(commit),
			diff: Rc::clone(diff),
		})
		.collect();

	Ok(Rc::new(commit_diffs))
}

pub(crate) fn commits_for_contributor(
	db: &dyn GitProvider,
	contributor: Rc<Contributor>,
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
				Some(Rc::clone(&all_commits[com_con.commit_id]))
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
	commit: Rc<Commit>,
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
			let author = Rc::clone(&contributors[com_con.author_id]);
			let committer = Rc::clone(&contributors[com_con.committer_id]);

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
	signer_name: Rc<String>,
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
				Some(Rc::clone(&all_commits[com_sign.commit_id]))
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
	signer_key: Rc<String>,
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
				Some(Rc::clone(&all_commits[com_sign.commit_id]))
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
	commit: Rc<Commit>,
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

pub(crate) fn signer_for_name(db: &dyn GitProvider, signer_name: Rc<String>) -> Result<SignerView> {
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

pub(crate) fn signer_for_key(db: &dyn GitProvider, signer_key: Rc<String>) -> Result<SignerView> {
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

pub(crate) fn get_short_hash(db: &dyn GitProvider, long_hash: Rc<String>) -> Result<String> {
	let repo = db.local();
	let repo_path = repo.as_path();
	let output = GitCommand::for_repo(repo_path, &["rev-parse", "--short", long_hash.as_ref()])?
		.output()
		.context("git rev-parse command failed")?;

	Ok(output)
}
