// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use hipcheck_sdk::{
	prelude::*,
	types::{LocalGitRepo, Target},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
	fmt::{self, Display, Formatter},
	result::Result as StdResult,
	sync::OnceLock,
};

#[derive(Deserialize)]
struct Config {
	#[serde(rename = "percent-threshold")]
	percent_threshold: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, JsonSchema)]
pub struct Commit {
	pub hash: String,
	pub written_on: StdResult<String, String>,
	pub committed_on: StdResult<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Contributor {
	pub name: String,
	pub email: String,
}

impl Display for Contributor {
	fn fmt(&self, f: &mut Formatter) -> fmt::Result {
		write!(f, "{} <{}>", self.name, self.email)
	}
}

/// Temporary data structure for looking up the contributors of a commit
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct CommitContributorView {
	pub commit: Commit,
	pub author: Contributor,
	pub committer: Contributor,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DetailedGitRepo {
	/// The local repo
	local: LocalGitRepo,
	/// Optional additional information for the query, hash in this case
	pub details: String,
}

impl Display for Commit {
	fn fmt(&self, f: &mut Formatter) -> fmt::Result {
		write!(f, "{}", self.hash)
	}
}

#[query]
async fn commit_identity(engine: &mut PluginEngine, key: DetailedGitRepo) -> Result<bool> {
	let value = engine
		.query("mitre/git/contributors_for_commit", key)
		.await
		.map_err(|e| {
			log::error!("failed to get last commits for identity metric: {}", e);
			Error::UnspecifiedQueryState
		})?;
	let ccv = serde_json::from_value::<CommitContributorView>(value)
		.map_err(Error::InvalidJsonInQueryOutput)?;
	Ok(ccv.author == ccv.committer)
}

#[query(default)]
async fn identity(engine: &mut PluginEngine, key: Target) -> Result<Vec<bool>> {
	// Get the commits for the source.
	let repo = key.local;
	let value = engine
		.query("mitre/git/commits", repo.clone())
		.await
		.map_err(|e| {
			log::error!("failed to get last commits for identity metric: {}", e);
			Error::UnspecifiedQueryState
		})?;
	let commits: Vec<Commit> =
		serde_json::from_value(value).map_err(|_| Error::UnexpectedPluginQueryInputFormat)?;
	let mut res = vec![];
	for c in commits {
		let key = DetailedGitRepo {
			local: repo.clone(),
			details: c.hash,
		};
		res.push(commit_identity(engine, key).await?);
	}
	Ok(res)
}

#[derive(Clone, Debug, Default)]
struct IdentityPlugin {
	policy_conf: OnceLock<Option<u64>>,
}

impl Plugin for IdentityPlugin {
	const PUBLISHER: &'static str = "mitre";
	const NAME: &'static str = "identity";

	fn set_config(&self, config: Value) -> StdResult<(), ConfigError> {
		// Deserialize the config struct
		let conf =
			serde_json::from_value::<Config>(config).map_err(|e| ConfigError::Unspecified {
				message: e.to_string(),
			})?;
		self.policy_conf
			.set(conf.percent_threshold)
			.map_err(|_| ConfigError::Unspecified {
				message: "plugin was already configured".to_string(),
			})?;
		Ok(())
	}

	fn default_policy_expr(&self) -> Result<String> {
		match self.policy_conf.get() {
			None => Err(Error::UnspecifiedQueryState),
			// If no policy vars, we have no default expr
			Some(None) => Ok("".to_owned()),
			// Use policy config vars to construct a default expr
			Some(Some(percent_threshold)) => Ok(format!(
				"(lte (divz (count (filter (eq #t) $)) (count $)) {})",
				percent_threshold
			)),
		}
	}

	fn explain_default_query(&self) -> Result<Option<String>> {
		Ok(Some(
			"Returns whether each commit in the repo was commited and authored by the same person"
				.to_owned(),
		))
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
	PluginServer::register(IdentityPlugin::default())
		.listen(args.port)
		.await
}

#[cfg(test)]
mod test {
	use super::*;

	fn target() -> Target {
		let local = LocalGitRepo {
			git_ref: "HEAD".to_owned(),
			path: ".".to_owned(),
		};
		Target {
			specifier: "".to_owned(),
			local,
			remote: None,
			package: None,
		}
	}

	fn mock() -> Result<MockResponses> {
		let target = target();
		let local = target.local.clone();
		let detailed1 = DetailedGitRepo {
			local: local.clone(),
			details: "abc123".to_owned(),
		};
		let detailed2 = DetailedGitRepo {
			local: local.clone(),
			details: "def456".to_owned(),
		};
		let committer = Contributor {
			name: "John Doe".to_owned(),
			email: "johndoe@gmail.com".to_owned(),
		};
		let author = Contributor {
			name: "Jane Doe".to_owned(),
			email: "janedoe@gmail.com".to_owned(),
		};
		let mut res = MockResponses::new();
		let commit1 = Commit {
			hash: "abc123".to_owned(),
			written_on: Ok("10/23/2024".to_owned()),
			committed_on: Ok("10/23/2024".to_owned()),
		};
		let commit2 = Commit {
			hash: "def456".to_owned(),
			written_on: Ok("10/23/2024".to_owned()),
			committed_on: Ok("10/23/2024".to_owned()),
		};
		let commits = vec![commit1.clone(), commit2.clone()];
		res.insert("mitre/git/commits", local, Ok(commits))?;
		res.insert(
			"mitre/git/contributors_for_commit",
			detailed1,
			Ok(CommitContributorView {
				commit: commit1.clone(),
				author: committer.clone(),
				committer: committer.clone(),
			}),
		)?;
		res.insert(
			"mitre/git/contributors_for_commit",
			detailed2,
			Ok(CommitContributorView {
				commit: commit2.clone(),
				author: author.clone(),
				committer: committer.clone(),
			}),
		)?;
		Ok(res)
	}

	#[tokio::test]
	async fn test_identity() {
		let mut engine = PluginEngine::mock(mock().unwrap());

		let res = identity(&mut engine, target()).await.unwrap();
		assert_eq!(vec![true, false], res);
	}
}
