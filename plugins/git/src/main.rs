// SPDX-License-Identifier: Apache-2.0

//! Plugin containing secondary queries that return information about a Git repo to another query

mod data;
mod git;

use crate::{
	data::{
		Commit, CommitContributor, CommitContributorView, CommitDiff, Contributor, ContributorView,
		DetailedGitRepo, Diff,
	},
	git::{
		get_all_raw_commits, get_commit_diffs, get_commits_from_date, get_contributors, get_diffs,
		get_latest_commit, GitRawCommitCache,
	},
};
use clap::Parser;
use hipcheck_sdk::{prelude::*, types::LocalGitRepo};
use jiff::Timestamp;
use lru::LruCache;
use serde::Deserialize;
use std::{
	num::NonZero,
	str::FromStr,
	sync::{Mutex, OnceLock},
};

pub static CACHE: OnceLock<Mutex<GitRawCommitCache>> = OnceLock::new();

/// Returns the date of the most recent commit to a Git repo as `jiff:Timestamp` displayed as a String
/// (Which means that anything expecting a `Timestamp` must parse the output of this query appropriately)
#[query]
async fn last_commit_date(_engine: &mut PluginEngine, repo: LocalGitRepo) -> Result<String> {
	let path = &repo.path;
	let last_commit = get_latest_commit(path)?;
	match last_commit {
		Some(commit) => match commit.written_on {
			Ok(date) => Ok(date.to_string()),
			Err(e) => {
				log::error!("{}", e);
				Err(Error::UnspecifiedQueryState)
			}
		},
		None => {
			log::error!("no commits");
			Err(Error::UnspecifiedQueryState)
		}
	}
}

/// Returns all diffs extracted from the repository
#[query]
async fn diffs(_engine: &mut PluginEngine, repo: LocalGitRepo) -> Result<Vec<Diff>> {
	let path = &repo.path;
	let diffs = get_diffs(path).map_err(|e| {
		log::error!("{}", e);
		Error::UnspecifiedQueryState
	})?;
	Ok(diffs)
}

/// Returns all commits extracted from the repository
#[query]
async fn commits(_engine: &mut PluginEngine, repo: LocalGitRepo) -> Result<Vec<Commit>> {
	let path = &repo.path;
	let raw_commits = get_all_raw_commits(path).map_err(|e| {
		log::error!("failed to get raw commits: {}", e);
		Error::UnspecifiedQueryState
	})?;
	let commits = raw_commits.into_iter().map(Commit::from).collect();

	Ok(commits)
}

/// Returns all commits extracted from the repository for a date given in the `details` field
/// The provided date must be of the form "YYYY-MM-DD"
#[query]
async fn commits_from_date(
	_engine: &mut PluginEngine,
	repo: DetailedGitRepo,
) -> Result<Vec<Commit>> {
	let path = &repo.local.path;
	let date = match repo.details {
		Some(date) => Timestamp::from_str(&date).map_err(|e| {
			log::error!("Failed to convert to jiff::Timestamp: {}", e);
			Error::UnspecifiedQueryState
		})?,
		None => {
			log::error!("No date provided");
			return Err(Error::UnspecifiedQueryState);
		}
	};
	// The called function will return an error if the date is not formatted correctly, so we do not need to check for ahead of time
	let raw_commits_from_date = get_commits_from_date(path, date).map_err(|e| {
		log::error!("failed to get raw commits from date: {}", e);
		Error::UnspecifiedQueryState
	})?;
	let commits = raw_commits_from_date
		.into_iter()
		.map(Commit::from)
		.collect();

	Ok(commits)
}

/// Returns all unique contributors to the repository
#[query]
async fn contributors(_engine: &mut PluginEngine, repo: LocalGitRepo) -> Result<Vec<Contributor>> {
	let path = &repo.path;
	let contributors = get_contributors(path).map_err(|e| {
		log::error!("failed to get contributors: {}", e);
		Error::UnspecifiedQueryState
	})?;
	Ok(contributors)
}

/// Returns all commit-diff pairs
#[query]
async fn commit_diffs(_engine: &mut PluginEngine, repo: LocalGitRepo) -> Result<Vec<CommitDiff>> {
	let commit_diffs = get_commit_diffs(&repo.path).map_err(|e| {
		log::error!("Error finding commit diffs: {}", e);
		Error::UnspecifiedQueryState
	})?;
	Ok(commit_diffs)
}

/// Returns the commits associated with a given contributor (identified by e-mail address in the `details` value)
#[query]
async fn commits_for_contributor(
	engine: &mut PluginEngine,
	repo: DetailedGitRepo,
) -> Result<ContributorView> {
	let local = repo.local;
	let email = match repo.details {
		Some(ref email) => email.clone(),
		None => {
			log::error!("No contributor e-mail address provided");
			return Err(Error::UnspecifiedQueryState);
		}
	};

	let all_commits = commits(engine, local.clone()).await.map_err(|e| {
		log::error!("failed to get commits: {}", e);
		Error::UnspecifiedQueryState
	})?;
	let contributors = contributors(engine, local.clone()).await.map_err(|e| {
		log::error!("failed to get contributors: {}", e);
		Error::UnspecifiedQueryState
	})?;
	let commit_contributors = commit_contributors(engine, local.clone())
		.await
		.map_err(|e| {
			log::error!("failed to get join table: {}", e);
			Error::UnspecifiedQueryState
		})?;

	// Get the index of the contributor
	let contributor_id = contributors
		.iter()
		.position(|c| c.email == email)
		.ok_or_else(|| {
			log::error!("failed to find contributor");
			Error::UnspecifiedQueryState
		})?;

	// Get the contributor
	let contributor = contributors[contributor_id].clone();

	// Find commits that have that contributor
	let commits = commit_contributors
		.iter()
		.filter_map(|com_con| {
			if com_con.author_id == contributor_id || com_con.committer_id == contributor_id {
				// SAFETY: This index is guaranteed to be valid in
				// `all_commits` because of how it and `commit_contributors`
				// are constructed from `db.raw_commits()`
				Some(all_commits[com_con.commit_id].clone())
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

/// Returns the contributor view for a given commit (idenftied by hash in the `details` field)
#[query]
async fn contributors_for_commit(
	engine: &mut PluginEngine,
	repo: DetailedGitRepo,
) -> Result<CommitContributorView> {
	let local = repo.local;
	let hash = match repo.details {
		Some(ref hash) => hash.clone(),
		None => {
			log::error!("No commit hash provided");
			return Err(Error::UnspecifiedQueryState);
		}
	};

	let commits = commits(engine, local.clone()).await.map_err(|e| {
		log::error!("failed to get commits: {}", e);
		Error::UnspecifiedQueryState
	})?;
	let contributors = contributors(engine, local.clone()).await.map_err(|e| {
		log::error!("failed to get contributors: {}", e);
		Error::UnspecifiedQueryState
	})?;
	let commit_contributors = commit_contributors(engine, local.clone())
		.await
		.map_err(|e| {
			log::error!("failed to get join table: {}", e);
			Error::UnspecifiedQueryState
		})?;

	// Get the index of the commit
	let commit_id = commits.iter().position(|c| c.hash == hash).ok_or_else(|| {
		log::error!("failed to find contributor");
		Error::UnspecifiedQueryState
	})?;

	// Get the commit
	let commit = commits[commit_id].clone();

	// Find the author and committer for that commit
	commit_contributors
		.iter()
		.find(|com_con| com_con.commit_id == commit_id)
		.map(|com_con| {
			// SAFETY: These indices are guaranteed to be valid in
			// `contributors` because of how `commit_contributors` is
			// constructed from it.
			let author = contributors[com_con.author_id].clone();
			let committer = contributors[com_con.committer_id].clone();

			CommitContributorView {
				commit,
				author,
				committer,
			}
		})
		.ok_or_else(|| {
			log::error!("failed to find contributor info");
			Error::UnspecifiedQueryState
		})
}

/// Internal use function that returns a join table of contributors by commit
async fn commit_contributors(
	engine: &mut PluginEngine,
	repo: LocalGitRepo,
) -> Result<Vec<CommitContributor>> {
	let path = &repo.path;
	let contributors = contributors(engine, repo.clone()).await.map_err(|e| {
		log::error!("failed to get contributors: {}", e);
		Error::UnspecifiedQueryState
	})?;
	let raw_commits = get_all_raw_commits(path).map_err(|e| {
		log::error!("failed to get raw commits: {}", e);
		Error::UnspecifiedQueryState
	})?;

	let commit_contributors = raw_commits
		.iter()
		.enumerate()
		.map(|(commit_id, raw)| {
			// SAFETY: These `position` calls are guaranteed to return `Some`
			// given how `contributors` is constructed from `db.raw_commits()`
			let author_id = contributors.iter().position(|c| c == &raw.author).unwrap();
			let committer_id = contributors
				.iter()
				.position(|c| c == &raw.committer)
				.unwrap();

			CommitContributor {
				commit_id,
				author_id,
				committer_id,
			}
		})
		.collect();

	Ok(commit_contributors)
}

/// Returns all of the contributors to a repo, along with how many times each contributor has made
/// a contribution. A contribution occurs when a contributor authors or commits a commit to the
/// repo.
#[query]
async fn contributor_summary(
	_engine: &mut PluginEngine,
	repo: LocalGitRepo,
) -> Result<Vec<CommitContributorView>> {
	let commits = get_all_raw_commits(repo.path).map_err(|e| {
		log::error!("failed to get commits: {}", e);
		Error::UnspecifiedQueryState
	})?;
	let mut commit_contributor_views = Vec::with_capacity(commits.len());
	for commit in commits {
		let view = CommitContributorView::from(commit);
		commit_contributor_views.push(view);
	}
	Ok(commit_contributor_views)
}
#[derive(Deserialize)]
struct Config {
	#[serde(default = "default_commit_cache_size")]
	commit_cache_size: usize,
}

fn default_commit_cache_size() -> usize {
	1
}

#[derive(Clone, Debug, Default)]
struct GitPlugin;

impl Plugin for GitPlugin {
	const PUBLISHER: &'static str = "mitre";

	const NAME: &'static str = "git";

	fn set_config(&self, config: Value) -> std::result::Result<(), ConfigError> {
		// Deserialize and validate the config struct
		let conf: Config =
			serde_json::from_value::<Config>(config).map_err(|e| ConfigError::Unspecified {
				message: e.to_string(),
			})?;
		let cache_size = conf.commit_cache_size;

		CACHE
			.set(Mutex::new(LruCache::new(NonZero::new(cache_size).unwrap())))
			.map_err(|_e| ConfigError::InternalError {
				message: "config was already set".to_owned(),
			})
	}

	fn default_policy_expr(&self) -> hipcheck_sdk::prelude::Result<String> {
		Ok("".to_owned())
	}

	fn explain_default_query(&self) -> hipcheck_sdk::prelude::Result<Option<String>> {
		Ok(None)
	}

	queries! {}
}

#[derive(Parser, Debug)]
struct Args {
	#[arg(long)]
	port: u16,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
	let args = Args::try_parse().unwrap();
	PluginServer::register(GitPlugin {}).listen(args.port).await
}
