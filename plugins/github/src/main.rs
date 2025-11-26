// SPDX-License-Identifier: Apache-2.0

mod cli;
mod config;
mod github;
mod tls;

use crate::{
	cli::Cli,
	config::CONFIG,
	github::{
		GitHub,
		graphql::{reviews::GitHubPullRequest, user_orgs::UserOrgData},
	},
};
use hipcheck_sdk::prelude::*;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
	let args = Cli::parse_args()?;

	PluginServer::register(GitHubPlugin, args.log_level)
		.listen_local(args.port)
		.await
}

/// Type representing the GitHub plugin.
#[derive(Debug)]
pub struct GitHubPlugin;

impl Plugin for GitHubPlugin {
	const PUBLISHER: &'static str = "mitre";
	const NAME: &'static str = "github";

	fn set_config(&self, config: Value) -> ConfigResult<()> {
		CONFIG.set(config)
	}

	queries! {
		pr_reviews,
		has_fuzz,
		user_orgs
	}
}

/// Get pull request review status for a repository.
#[query]
async fn pr_reviews(_e: &mut PluginEngine, repo: RemoteGitRepo) -> Result<Vec<GitHubPullRequest>> {
	let Some(KnownRemote::GitHub { owner, repo }) = &repo.known_remote else {
		return Err(Error::InvalidQueryTargetFormat);
	};

	let token = CONFIG.api_token()?;
	let github = GitHub::new(token)?;
	let reviews = github.get_reviews(owner, repo)?;
	Ok(reviews)
}

/// Check if a repository is enrolled with OSS-Fuzz.
#[query]
async fn has_fuzz(_e: &mut PluginEngine, repo: RemoteGitRepo) -> Result<bool> {
	let token = CONFIG.api_token()?;
	let github = GitHub::new(token)?;
	let fuzzing = github.check_fuzzing(repo.url.as_str())?;
	Ok(fuzzing)
}

/// Get the organizations to which a user belongs.
#[query]
async fn user_orgs(_e: &mut PluginEngine, user_login: String) -> Result<UserOrgData> {
	let token = CONFIG.api_token()?;
	let github = GitHub::new(token)?;
	let user_orgs = github.get_user_orgs(&user_login)?;
	Ok(user_orgs)
}
