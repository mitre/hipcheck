// SPDX-License-Identifier: Apache-2.0

//! Plugin for querying a repo for any contributors with concerning affiliations

mod affiliator;
mod config;
mod enriched_contributor;
mod git;
mod github;
mod org_spec;
mod org_types;
mod util;

use crate::{
	affiliator::Affiliator,
	config::Config,
	enriched_contributor::EnrichedContributor,
	git::GitCommitContributors,
	github::{GitHubCollaborator, GitHubRepoContributor},
	org_spec::OrgSpec,
	util::fs as file,
};
use clap::Parser;
use hipcheck_sdk::{LogLevel, PluginConfig, prelude::*, types::Target};
use std::{collections::HashMap, ops::Not, result::Result as StdResult, sync::OnceLock};

// We only keep a single org-spec in memory and just make it a global.
pub static ORGSSPEC: OnceLock<OrgSpec> = OnceLock::new();

/// Returns a boolean list with one entry per contributor to the repo
/// A `true` entry corresponds to an affiliated contributor
#[query]
async fn affiliation(engine: &mut PluginEngine, key: Target) -> Result<Vec<bool>> {
	tracing::info!("running affiliation query");

	let org_spec = &ORGSSPEC.get().ok_or_else(|| {
		tracing::error!("tried to access config before set by Hipcheck core!");
		Error::UnspecifiedQueryState
	})?;

	let repo = key.local;

	let git_commit_contributors = engine.query("mitre/git/contributor_summary", &repo).await?;
	let git_commit_contributors: Vec<GitCommitContributors> =
		serde_json::from_value(git_commit_contributors).map_err(|e| {
			tracing::error!("Error parsing output from mitre/git/contributor_summary: {e}");
			Error::UnexpectedPluginQueryInputFormat
		})?;

	let affiliator = Affiliator::from_spec(org_spec).map_err(|e| {
		tracing::error!("failed to build affiliation checker from org spec: {}", e);
		Error::UnspecifiedQueryState
	})?;

	// If we have a GitHub repo on our hands...
	let repo_collaborators = if let Some(
		ref repo @ RemoteGitRepo {
			known_remote: Some(KnownRemote::GitHub { .. }),
			..
		},
	) = key.remote
	{
		let repo_collaborators = engine
			.query("mitre/github/repo_collaborators", repo)
			.await?;
		let repo_collaborators: Vec<GitHubCollaborator> =
			serde_json::from_value(repo_collaborators).map_err(|e| {
				tracing::error!("Error parsing output from mitre/github/repo_collaborators: {e}");
				Error::UnspecifiedQueryState
			})?;
		let repo_collaborators: HashMap<String, GitHubCollaborator> = repo_collaborators
			.into_iter()
			.map(|collaborator| (collaborator.email.clone(), collaborator))
			.collect();

		repo_collaborators
	} else {
		HashMap::new()
	};

	let github_repo_contributors: HashMap<String, GitHubRepoContributor> = if let Some(
		ref repo @ RemoteGitRepo {
			known_remote: Some(KnownRemote::GitHub { .. }),
			..
		},
	) = key.remote
	{
		let github_repo_contributors = engine.query("mitre/github/repo_contributors", repo).await?;
		let github_repo_contributors: Vec<GitHubRepoContributor> =
			serde_json::from_value(github_repo_contributors).map_err(|e| {
				tracing::error!("Error parsing output from mitre/github/repo_contributors: {e}");
				Error::UnspecifiedQueryState
			})?;
		github_repo_contributors
			.into_iter()
			.filter_map(|contributor| {
				contributor
					.email
					.as_ref()
					.map(|email| (email.clone(), contributor.clone()))
			})
			.collect()
	} else {
		HashMap::new()
	};

	// So now that we have repo collaborators, we ought to enrich the git contributors with that
	// information if possible.
	//
	// Note that git_commit_contributors organizes per-commit, but what we basically want is to
	// organize per-contributor. So we build up a HashMap of Git identities to the fully-enriched
	// information, with the commits bundled up in that enriched structure.
	let mut contributors: HashMap<String, EnrichedContributor> = HashMap::new();

	for git_commit_contributor in git_commit_contributors {
		let commit = git_commit_contributor.commit.clone();

		// Add or update the commit author.
		contributors
			.entry(git_commit_contributor.author().email.trim().to_string())
			.and_modify(|enriched| {
				let new_commit = commit;
				if enriched.commits.contains(&new_commit).not() {
					enriched.commits.push(new_commit)
				}
			})
			.or_insert({
				let contributor = git_commit_contributor.clone();
				let author = contributor.author.clone();

				EnrichedContributor {
					git: contributor.author,
					github: repo_collaborators.get(&author.email).cloned(),
					commits: vec![contributor.commit],
				}
			});

		let commit = git_commit_contributor.commit.clone();

		// Add or update the commit committer.
		contributors
			.entry(git_commit_contributor.committer().email.trim().to_string())
			.and_modify(|enriched| {
				let new_commit = commit.clone();
				if enriched.commits.contains(&new_commit).not() {
					enriched.commits.push(new_commit)
				}
			})
			.or_insert({
				let contributor = git_commit_contributor.clone();
				let committer = contributor.committer.clone();

				EnrichedContributor {
					git: contributor.author,
					github: repo_collaborators.get(&committer.email).cloned(),
					commits: vec![contributor.commit],
				}
			});
	}

	for (email, enriched) in &mut contributors {
		if let Some(github_repo_contributor) = github_repo_contributors.get(email)
			&& let Some(email) = &github_repo_contributor.email
				&& let Some(login) = &github_repo_contributor.login
				&& enriched.github.is_none()
			{
				enriched.github = Some(GitHubCollaborator {
					login: login.clone(),
					email: email.clone(),
					profile_employer: None,
					github_orgs: vec![],
				})
			}
	}

	// TODO: Fix deduplication.

	// Drop the mutability by rebinding.
	let contributors = contributors;

	let mut affiliations = vec![false; contributors.len()];
	for (idx, contributor) in contributors.values().enumerate() {
		if affiliator.is_match(contributor) {
			let num_commits = contributor.commits.len();
			let pluralized_commits = if num_commits == 1 {
				"commit"
			} else {
				"commits"
			};
			let concern = format!(
				"Contributor {} ({}) wrote and/or committed {} {}",
				contributor.git.name, contributor.git.email, num_commits, pluralized_commits
			);
			engine.record_concern(concern);
			// SAFETY: affiliations has the same length as contributor_freq_map
			*affiliations.get_mut(idx).unwrap() = true;
		}
	}

	tracing::info!("completed affiliation query");
	Ok(affiliations)
}

#[derive(Clone, Debug, Default)]
struct AffiliationPlugin {
	policy_conf: OnceLock<Option<u64>>,
}

impl Plugin for AffiliationPlugin {
	const PUBLISHER: &'static str = "mitre";

	const NAME: &'static str = "affiliation";

	fn set_config(&self, config: Value) -> StdResult<(), ConfigError> {
		// Deserialize and validate the config struct
		let conf = Config::deserialize(&config)?;

		file::exists(&conf.orgs_file).map_err(|_e| ConfigError::FileNotFound {
			file_path: conf.orgs_file.to_string_lossy().into(),
		})?;

		// Parse the Orgs file and construct an OrgSpec.
		let orgs_spec =
			OrgSpec::load_from(&conf.orgs_file).map_err(|e| ConfigError::ParseError {
				source: format!("OrgSpec file at {}", conf.orgs_file.to_string_lossy())
					.into_boxed_str(),
				// Print error with Debug for full context
				message: format!("{:?}", e).into_boxed_str(),
			})?;

		// Store the policy conf to be accessed only in the `default_policy_expr()` impl
		self.policy_conf
			.set(conf.count_threshold)
			.map_err(|_| ConfigError::InternalError {
				message: "plugin was already configured".to_string().into_boxed_str(),
			})?;

		ORGSSPEC
			.set(orgs_spec)
			.map_err(|_e| ConfigError::InternalError {
				message: "orgs spec was already set".to_owned().into_boxed_str(),
			})
	}

	fn default_policy_expr(&self) -> Result<String> {
		match self.policy_conf.get() {
			None => Err(Error::UnspecifiedQueryState),
			// If no policy vars, we have no default expr
			Some(None) => Ok("".to_owned()),
			// Use policy config vars to construct a default expr
			Some(Some(policy_conf)) => {
				Ok(format!("(lte (count (filter (eq #t) $)) {})", policy_conf))
			}
		}
	}

	fn explain_default_query(&self) -> Result<Option<String>> {
		Ok(Some(
			"the repository's contributors flagged as affiliated".to_string(),
		))
	}

	queries! {
		#[default] affiliation
	}
}

#[derive(Parser, Debug)]
struct Args {
	#[arg(long)]
	port: u16,

	#[arg(long, default_value_t=LogLevel::Error)]
	log_level: LogLevel,

	#[arg(trailing_var_arg(true), allow_hyphen_values(true), hide = true)]
	unknown_args: Vec<String>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
	let args = Args::try_parse().unwrap();
	PluginServer::register(AffiliationPlugin::default(), args.log_level)
		.listen_local(args.port)
		.await
}

#[cfg(test)]
mod test {
	use super::git::{Commit, GitIdentity};
	use super::*;
	use hipcheck_sdk::types::LocalGitRepo;
	use pathbuf::pathbuf;
	use std::{env, result::Result as StdResult};

	fn repo() -> LocalGitRepo {
		LocalGitRepo {
			path: "/home/users/me/.cache/hipcheck/clones/github/foo/bar/".to_string(),
			git_ref: "main".to_string(),
		}
	}

	fn mock_responses() -> StdResult<MockResponses, Error> {
		let repo = repo();

		let commit_1 = Commit {
			hash: "abc-123".to_string(),
			written_on: Ok("2024-06-19T20:00:00Z".to_string()),
			committed_on: Ok("2024-06-19T21:00:00Z".to_string()),
		};

		let commit_2 = Commit {
			hash: "def-456".to_string(),
			written_on: Ok("2024-06-20T20:00:00Z".to_string()),
			committed_on: Ok("2024-06-20T21:00:00Z".to_string()),
		};

		let commit_3 = Commit {
			hash: "ghi-789".to_string(),
			written_on: Ok("2024-06-21T20:00:00Z".to_string()),
			committed_on: Ok("2024-06-21T21:00:00Z".to_string()),
		};

		let contributor_1 = GitIdentity {
			name: "John Smith".to_string(),
			email: "jsmith@mitre.org".to_string(),
		};

		let contributor_2 = GitIdentity {
			name: "Jane Doe".to_string(),
			email: "jdoe@gmail.com".to_string(),
		};

		let commit_1_view = GitCommitContributors {
			commit: commit_1.clone(),
			author: contributor_1.clone(),
			committer: contributor_1.clone(),
		};

		let commit_2_view = GitCommitContributors {
			commit: commit_2.clone(),
			author: contributor_2.clone(),
			committer: contributor_1.clone(),
		};

		let commit_3_view = GitCommitContributors {
			commit: commit_3.clone(),
			author: contributor_2.clone(),
			committer: contributor_2.clone(),
		};

		let mut mock_responses = MockResponses::new();
		mock_responses
			.insert(
				"mitre/git/contributor_summary",
				repo.clone(),
				Ok(vec![commit_1_view, commit_2_view, commit_3_view]),
			)
			.unwrap();
		Ok(mock_responses)
	}

	#[tokio::test]
	async fn test_affiliation() {
		let orgs_file = pathbuf![&env::current_dir().unwrap(), "test", "test_orgs.kdl"];
		let orgs_spec = OrgSpec::load_from(&orgs_file).unwrap();
		ORGSSPEC.get_or_init(|| orgs_spec);

		let repo = repo();
		let target = Target {
			specifier: "bar".to_string(),
			local: repo,
			remote: None,
			package: None,
		};

		let mut engine = PluginEngine::mock(mock_responses().unwrap());
		let output = affiliation(&mut engine, target).await.unwrap();
		let concerns = engine.get_concerns();
		assert_eq!(output.len(), 2);
		let num_affiliated = output.iter().filter(|&n| *n).count();
		assert_eq!(num_affiliated, 1);
		assert_eq!(
			concerns[0],
			"Contributor Jane Doe (jdoe@gmail.com) wrote and/or committed 2 commits"
		)
	}
}
