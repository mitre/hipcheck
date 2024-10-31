// SPDX-License-Identifier: Apache-2.0

//! Plugin for querying a repo for any contributors with concerning affiliations

mod org_spec;
mod org_types;
mod util;

use crate::{
	org_spec::{Matcher, OrgSpec},
	org_types::Mode,
	util::fs as file,
};

use clap::Parser;
use hipcheck_sdk::{
	prelude::*,
	types::{LocalGitRepo, Target},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
	collections::{HashMap, HashSet},
	fmt::{self, Display, Formatter},
	path::PathBuf,
	result::Result as StdResult,
	sync::OnceLock,
};

#[derive(Debug, Deserialize)]
struct Config {
	orgs_spec: OrgSpec,

	// Maximum number of concerningly affilaited contributors permitted in a default query
	count_threshold: Option<u16>,
}

#[derive(Deserialize)]
struct RawConfig {
	orgs_file_var: Option<String>,
	count_threshold_var: Option<u16>,
}

impl TryFrom<RawConfig> for Config {
	type Error = ConfigError;
	fn try_from(value: RawConfig) -> StdResult<Config, ConfigError> {
		if let Some(ofv) = value.orgs_file_var {
			// Get the Orgs file path and confirm it exists
			let orgs_file = PathBuf::from(&ofv);
			file::exists(&orgs_file).map_err(|_e| ConfigError::InvalidConfigValue {
				field_name: "orgs_file_var".to_owned(),
				value: ofv.clone(),
				reason: "could not find an orgs file with that name".to_owned(),
			})?;
			// Parse the Orgs file and construct an OrgSpec.
			let orgs_spec =
				OrgSpec::load_from(&orgs_file).map_err(|e| ConfigError::InvalidConfigValue {
					field_name: "orgs_file_var".to_owned(),
					value: ofv.clone(),
					reason: format!("Failed to load org spec: {}", e),
				})?;
			Ok(Config {
				orgs_spec,
				count_threshold: value.count_threshold_var,
			})
		} else {
			Err(ConfigError::MissingRequiredConfig {
				field_name: "orgs_file_var".to_owned(),
				field_type: "name of env var containing GitHub API token".to_owned(),
				possible_values: vec![],
			})
		}
	}
}

static CONFIG: OnceLock<Config> = OnceLock::new();

/// A locally stored git repo, with optional additional details
/// The details will vary based on the query (e.g. a date, a committer e-mail address, a commit hash)
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct DetailedGitRepo {
	/// The local repo
	local: LocalGitRepo,

	/// Optional additional information for the query
	pub details: Option<String>,
}

/// Commits as understood in Hipcheck's data model.
/// The `written_on` and `committed_on` datetime fields contain Strings that are created from `jiff:Timestamps`.
/// Because `Timestamp` does not `impl JsonSchema`, we display the datetimes as Strings for passing out of this plugin.
/// Other plugins that expect a `Timestamp`` should parse the provided Strings into `Timestamps` as needed.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, JsonSchema)]
pub struct Commit {
	pub hash: String,

	pub written_on: StdResult<String, String>,

	pub committed_on: StdResult<String, String>,
}

impl Display for Commit {
	fn fmt(&self, f: &mut Formatter) -> fmt::Result {
		write!(f, "{}", self.hash)
	}
}

/// Authors or committers of a commit.
#[derive(
	Debug, PartialEq, Eq, Deserialize, Serialize, Clone, Hash, PartialOrd, Ord, JsonSchema,
)]
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
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq, JsonSchema)]
pub struct CommitContributorView {
	pub commit: Commit,
	pub author: Contributor,
	pub committer: Contributor,
}

/// Temporary data structure for looking up the commits associated with a contributor
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq, JsonSchema)]
pub struct ContributorView {
	pub contributor: Contributor,
	pub commits: Vec<Commit>,
}

/// A type which encapsulates checking whether a given string matches an org in the orgs file,
/// based on the mode in question. If the mode is Independent, then you're looking for
/// the strings that _don't match_ any of the hosts in the set. If the mode is Affiliated,
/// you're looking for the strings that _match_ one of the hosts in the set.
struct Affiliator<'haystack> {
	patterns: Matcher<'haystack>,
	mode: Mode,
}

impl<'haystack> Affiliator<'haystack> {
	/// Check whether the given string is a match for the set of hosts, based on the mode.
	///
	/// If independent mode is on, you're looking for strings which do not match any of
	/// the hosts.
	///
	/// If affiliated mode is on, you're looking for strings which do match one of the
	/// hosts.
	fn is_match(&self, s: &str) -> bool {
		match self.mode {
			Mode::Independent => !self.patterns.is_match(s),
			Mode::Affiliated => self.patterns.is_match(s),
			Mode::All => true,
			Mode::None => false,
		}
	}

	/// Construct a new Affiliator from a given OrgSpec (built from an Orgs.kdl file).
	fn from_spec(spec: &'haystack OrgSpec) -> Result<Affiliator<'haystack>> {
		let patterns = spec.patterns().map_err(|e| {
			log::error!("failed to get patterns for org spec to check against {}", e);
			Error::UnspecifiedQueryState
		})?;
		let mode = spec.mode();
		Ok(Affiliator { patterns, mode })
	}
}

/// A commit and which of its contributors meets the affiliation criteria
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct AffiliationDetails {
	pub commit: Commit,
	pub affiliated_type: AffiliatedType,
}

#[derive(Debug, Eq, PartialEq, Serialize, Clone, Copy)]
pub enum AffiliatedType {
	Author,
	Committer,
	Both,
	Neither,
}

impl AffiliatedType {
	fn is(affiliator: &Affiliator, commit_view: &CommitContributorView) -> AffiliatedType {
		let author_is_match = affiliator.is_match(&commit_view.author.email);
		let committer_is_match = affiliator.is_match(&commit_view.committer.email);

		match (author_is_match, committer_is_match) {
			(true, true) => AffiliatedType::Both,
			(true, false) => AffiliatedType::Author,
			(false, true) => AffiliatedType::Committer,
			(false, false) => AffiliatedType::Neither,
		}
	}

	pub fn is_affiliated(&self) -> bool {
		!matches!(self, AffiliatedType::Neither)
	}
}

/// Returns the number of commits that are flagged for having concerning contributors
#[query]
async fn affiliation(engine: &mut PluginEngine, key: Target) -> Result<i64> {
	log::debug!("running affiliation query");

	// Get the OrgSpec.
	let org_spec = &CONFIG
		.get()
		.ok_or_else(|| {
			log::error!("tried to access config before set by Hipcheck core!");
			Error::UnspecifiedQueryState
		})?
		.orgs_spec;

	// Get the commits for the source.
	let repo = key.local;
	let value = engine
		.query("mitre/git/commits", repo.clone())
		.await
		.map_err(|e| {
			log::error!("failed to get last commits for affiliation metric: {}", e);
			Error::UnspecifiedQueryState
		})?;
	let commits: Vec<Commit> =
		serde_json::from_value(value).map_err(|_| Error::UnexpectedPluginQueryInputFormat)?;

	// Use the OrgSpec to build an Affiliator.
	let affiliator = Affiliator::from_spec(org_spec).map_err(|e| {
		log::error!("failed to build affiliation checker from org spec: {}", e);
		Error::UnspecifiedQueryState
	})?;

	// Construct a big enough Vec for the affiliation info.
	let mut affiliations = Vec::with_capacity(commits.len());

	for commit in commits.iter() {
		// Check if a commit matches the affiliation rules.
		let hash = commit.hash.clone();
		let detailed_repo = DetailedGitRepo {
			local: repo.clone(),
			details: Some(hash.clone()),
		};
		let view_value = engine
			.query("mitre/git/contributors_for_commit", detailed_repo)
			.await
			.map_err(|e| {
				log::error!("failed to get contributors for commit {}: {}", hash, e);
				Error::UnspecifiedQueryState
			})?;
		let commit_view: CommitContributorView = serde_json::from_value(view_value)
			.map_err(|_| Error::UnexpectedPluginQueryInputFormat)?;

		let affiliated_type = AffiliatedType::is(&affiliator, &commit_view);

		affiliations.push(AffiliationDetails {
			commit: commit.clone(),
			affiliated_type,
		});
	}

	let affiliated_iter = affiliations
		.into_iter()
		.filter(|a| a.affiliated_type.is_affiliated());

	let mut contributors = HashSet::new();
	let mut contributor_freq_map = HashMap::new();

	// Get the affiliated contributors from the commits
	for affiliation in affiliated_iter {
		let hash = affiliation.commit.hash;
		let commit_repo = DetailedGitRepo {
			local: repo.clone(),
			details: Some(hash.clone()),
		};
		let view_value = engine
			.query("mitre/git/contributors_for_commit", commit_repo)
			.await
			.map_err(|e| {
				log::error!("failed to get contributors for commit {}: {}", hash, e);
				Error::UnspecifiedQueryState
			})?;
		let commit_view: CommitContributorView = serde_json::from_value(view_value)
			.map_err(|_| Error::UnexpectedPluginQueryInputFormat)?;

		match affiliation.affiliated_type {
			AffiliatedType::Author => {
				contributors.insert((commit_view.author.name, commit_view.author.email));
			}
			AffiliatedType::Committer => {
				contributors.insert((commit_view.committer.name, commit_view.committer.email));
			}
			AffiliatedType::Neither => (),
			// Add both author and committer to the hash set if both are affiliated, in case they are distinct
			AffiliatedType::Both => {
				contributors.insert((commit_view.author.name, commit_view.author.email));
				contributors.insert((commit_view.committer.name, commit_view.committer.email));
			}
		};
	}

	// Add string representation of affiliated contributor with count of associated commits
	for contributor in contributors.into_iter() {
		let count = count_commits_for(engine, repo.clone(), contributor.1).await?;
		contributor_freq_map.insert(contributor.0, count);
	}

	// Get the total number of affiliated contributors
	let count = contributor_freq_map.keys().count() as i64;

	// Add each contributor-count pair as a concern
	for (contributor, count) in contributor_freq_map.into_iter() {
		let concern = format!("Contributor {} has count {}", contributor, count);
		engine.record_concern(concern);
	}

	log::info!("completed affiliation metric");

	Ok(count)
}

/// Gets the number of commits to a repo associated with a given contributor
async fn count_commits_for(
	engine: &mut PluginEngine,
	repo: LocalGitRepo,
	email: String,
) -> Result<i64> {
	let contributor_repo = DetailedGitRepo {
		local: repo,
		details: Some(email.clone()),
	};
	let contributor_value = engine
		.query("mitre/git/commits_for_contributor", contributor_repo)
		.await
		.map_err(|e| {
			log::error!("failed to get commits for contributor {}: {}", email, e);
			Error::UnspecifiedQueryState
		})?;

	let contributor_view: ContributorView = serde_json::from_value(contributor_value)
		.map_err(|_| Error::UnexpectedPluginQueryInputFormat)?;
	Ok(contributor_view.commits.len() as i64)
}

#[derive(Clone, Debug)]
struct AffiliationPlugin;

impl Plugin for AffiliationPlugin {
	const PUBLISHER: &'static str = "mitre";

	const NAME: &'static str = "affiliation";

	fn set_config(&self, config: Value) -> StdResult<(), ConfigError> {
		let conf =
			serde_json::from_value::<Config>(config).map_err(|e| ConfigError::Unspecified {
				message: e.to_string(),
			})?;
		CONFIG.set(conf).map_err(|_e| ConfigError::Unspecified {
			message: "config was already set".to_owned(),
		})
	}

	fn default_policy_expr(&self) -> Result<String> {
		let Some(conf) = CONFIG.get() else {
			log::error!("tried to access config before set by Hipcheck core!");
			return Err(Error::UnspecifiedQueryState);
		};
		match conf.count_threshold {
			Some(threshold) => Ok(format!("lte $ {}", threshold)),
			None => Ok("".to_owned()),
		}
	}

	fn explain_default_query(&self) -> Result<Option<String>> {
		Ok(Some(
			"Number of affiliated committers to permit".to_string(),
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
	PluginServer::register(AffiliationPlugin {})
		.listen(args.port)
		.await
}

#[cfg(test)]
mod test {
	use super::*;

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

		let contributor_1 = Contributor {
			name: "John Smith".to_string(),
			email: "jsmith@mitre.org".to_string(),
		};

		let contributor_2 = Contributor {
			name: "Jane Doe".to_string(),
			email: "jdoe@gmail.com".to_string(),
		};

		let commit_1_view = CommitContributorView {
			commit: commit_1.clone(),
			author: contributor_1.clone(),
			committer: contributor_1.clone(),
		};

		let commit_2_view = CommitContributorView {
			commit: commit_2.clone(),
			author: contributor_2.clone(),
			committer: contributor_1.clone(),
		};

		let commit_3_view = CommitContributorView {
			commit: commit_3.clone(),
			author: contributor_2.clone(),
			committer: contributor_2.clone(),
		};

		let contributor_1_view = ContributorView {
			contributor: contributor_1.clone(),
			commits: vec![commit_1.clone(), commit_2.clone()],
		};

		let contributor_2_view = ContributorView {
			contributor: contributor_2.clone(),
			commits: vec![commit_2.clone(), commit_3.clone()],
		};

		let commit_1_repo = DetailedGitRepo {
			local: repo.clone(),
			details: Some("abc-123".to_string()),
		};

		let commit_2_repo = DetailedGitRepo {
			local: repo.clone(),
			details: Some("def-456".to_string()),
		};

		let commit_3_repo = DetailedGitRepo {
			local: repo.clone(),
			details: Some("ghi-789".to_string()),
		};

		let contributor_1_repo = DetailedGitRepo {
			local: repo.clone(),
			details: Some("jsmith@mitre.org".to_string()),
		};

		let contributor_2_repo = DetailedGitRepo {
			local: repo.clone(),
			details: Some("jdoe@gmail.com".to_string()),
		};

		let mut mock_responses = MockResponses::new();

		mock_responses
			.insert(
				"mitre/git/commits",
				repo,
				Ok(vec![commit_1, commit_2, commit_3]),
			)
			.unwrap();
		mock_responses
			.insert(
				"mitre/git/contributors_for_commit",
				commit_1_repo,
				Ok(commit_1_view),
			)
			.unwrap();
		mock_responses
			.insert(
				"mitre/git/contributors_for_commit",
				commit_2_repo,
				Ok(commit_2_view),
			)
			.unwrap();
		mock_responses
			.insert(
				"mitre/git/contributors_for_commit",
				commit_3_repo,
				Ok(commit_3_view),
			)
			.unwrap();
		mock_responses
			.insert(
				"mitre/git/commits_for_contributor",
				contributor_1_repo,
				Ok(contributor_1_view),
			)
			.unwrap();
		mock_responses
			.insert(
				"mitre/git/commits_for_contributor",
				contributor_2_repo,
				Ok(contributor_2_view),
			)
			.unwrap();

		Ok(mock_responses)
	}

	#[tokio::test]
	async fn test_affiliation() {
		let orgs_file = pathbuf![&env::current_dir().unwrap(), "test", "example_orgs.kdl"];
		let orgs_spec = OrgSpec::load_from(&orgs_file).unwrap();
		let conf = Config {
			orgs_spec,
			count_threshold: None,
		};
		CONFIG.get_or_init(|| conf);

		let repo = repo();
		let target = Target {
			specifier: "bar".to_string(),
			local: repo,
			remote: None,
			package: None,
		};

		let mut engine = PluginEngine::mock(mock_responses().unwrap());
		let output = affiliation(&mut engine, target).await.unwrap();

		let concerns = engine.take_concerns();

		assert_eq!(output, 1);
		assert_eq!(concerns[0], "Contributor Jane Doe has count 2")
	}
}
