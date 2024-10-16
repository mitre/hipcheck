// SPDX-License-Identifier: Apache-2.0

//! Plugin for querying a repo for any contributors with concerning affiliations

mod org_spec;
mod org_types;
mod util;

use crate::{util::fs as file, org_spec::{Matcher, OrgSpec}, org_types::Mode};

use clap::Parser;
use hipcheck_sdk::{prelude::*, types::{LocalGitRepo, Target}};
use schemars::JsonSchema;
use serde::{Serialize, Deserialize};
use std::{fmt::{self, Display, Formatter}, path::PathBuf, result::Result as StdResult, sync::OnceLock};

#[derive(Deserialize)]
struct Config {
	orgs_file: PathBuf,
	
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
			let orgs_file = PathBuf::from(&ofv);
			file::exists(&orgs_file).map_err(|_e| ConfigError::InvalidConfigValue {
				field_name: "orgs_file_var".to_owned(),
				value: ofv,
				reason: "could not find an orgs file with that name".to_owned(),
			})?;
			Ok(Config { orgs_file, count_threshold: value.count_threshold_var })
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
#[derive(Debug, Deserialize, Clone, PartialEq, Eq, Hash, JsonSchema)]
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
	Debug, PartialEq, Eq, Deserialize, Clone, Hash, PartialOrd, Ord, JsonSchema,
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
#[derive(Debug, Deserialize, Clone, PartialEq, Eq, JsonSchema)]
pub struct CommitContributorView {
	pub commit: Commit,
	pub author: Contributor,
	pub committer: Contributor,
}

/// A type which encapsulates checking whether a given string matches an org in theorgs file,
/// based on the mode in question. If the mode is Independent, then you're looking for
/// the strings that don't match any of the hosts in the set. If the mode is Affiliated,
/// you're looking for the strings that match one of the hosts in the set.
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

	/// Construct a new Affiliator from a given OrgSpec (built from an Orgs.toml file).
	fn from_spec(spec: &'haystack OrgSpec) -> Result<Affiliator<'haystack>> {
		let patterns = spec.patterns().map_err(|e| {
			log::error!("failed to get patterns for org spec to check against {}", e);
			Error::UnspecifiedQueryState
		})?;
		let mode = spec.mode();
		Ok(Affiliator { patterns, mode })
	}
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct Affiliation {
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

	// Parse the Orgs file and construct an OrgSpec.
	let org_spec_path = &CONFIG
	.get()
	.ok_or_else(|| {
		log::error!("tried to access config before set by Hipcheck core!");
		Error::UnspecifiedQueryState
	})?
	.orgs_file;	
	let org_spec = OrgSpec::load_from(org_spec_path).map_err(|e| {
		log::error!("failed to load org spec: {}", e);
		Error::UnspecifiedQueryState
	})?;

	// Get the commits for the source.
	let repo = key.local;
	let value = engine
	.query("mitre/git/commits", repo)
	.await
	.map_err(|e| {
		log::error!("failed to get last commits for affiliation metric: {}", e);
		Error::UnspecifiedQueryState
	})?;
	let commits: Vec<Commit> = serde_json::from_value(value).map_err(|_| Error::UnexpectedPluginQueryInputFormat)?;

	// Use the OrgSpec to build an Affiliator.
	let affiliator = Affiliator::from_spec(&org_spec)
		.map_err(|e| {
			log::error!("failed to build affiliation checker from org spec: {}", e);
			Error::UnspecifiedQueryState
		})?;

	// Construct a big enough Vec for the affiliation info.
	let mut affiliations = Vec::with_capacity(commits.len());

	for commit in commits.iter() {
		// Check if a commit matches the affiliation rules.
		let hash = commit.hash;
		let detailed_repo = DetailedGitRepo {
			local: repo,
			details: Some(hash),
		};
		let view_value = engine
			.query("mitre/git/contributors_for_commit", detailed_repo)
			.await
			.map_err(|e| {
				log::error!("failed to get contributors for commit {}: {}", hash, e);
				Error::UnspecifiedQueryState
			})?;
		let commit_view: CommitContributorView = serde_json::from_value(value).map_err(|_| Error::UnexpectedPluginQueryInputFormat)?;

		let affiliated_type = AffiliatedType::is(&affiliator, &commit_view);

		affiliations.push(Affiliation {
			commit: commit.clone(),
			affiliated_type,
		});
	}

	let affiliated_iter = affiliations
	.into_iter()
	.filter(|a| a.affiliated_type.is_affiliated());

	// @Note - policy expr json injection can't handle objs/strings currently
	let value: Vec<bool> = affiliated_iter.clone().map(|_| true).collect();
	let count = value.len();

	let mut contributor_freq_map = HashMap::new();

	for affiliation in affiliated_iter {
		let view_value = engine
			.query("mitre/git/contributors_for_commit", detailed_repo)
			.await
			.map_err(|e| {
				log::error!("failed to get contributors for commit {}: {}", hash, e);
				Error::UnspecifiedQueryState
			})?;
		let commit_view: CommitContributorView = serde_json::from_value(value).map_err(|_| Error::UnexpectedPluginQueryInputFormat)?;

		let contributor = match affiliation.affiliated_type {
			AffiliatedType::Author => String::from(&commit_view.author.name),
			AffiliatedType::Committer => String::from(&commit_view.committer.name),
			AffiliatedType::Neither => String::from("Neither"),
			AffiliatedType::Both => String::from("Both"),
		};

		let count_commits_for = |contributor| {
			db.commits_for_contributor(Arc::clone(contributor))
				.into_iter()
				.count() as i64
		};

		let author_commits = count_commits_for(&commit_view.author);
		let committer_commits = count_commits_for(&commit_view.committer);

		let commit_count = match affiliation.affiliated_type {
			AffiliatedType::Neither => 0,
			AffiliatedType::Both => author_commits + committer_commits,
			AffiliatedType::Author => author_commits,
			AffiliatedType::Committer => committer_commits,
		};

		// Add string representation of affiliated contributor with count of associated commits
		contributor_freq_map.insert(contributor, commit_count);
	}

	let concerns: Vec<String> = contributor_freq_map
		.into_iter()
		.map(|(contributor, count)| format!("Contributor {} has count {}", contributor, count))
		.collect();

	// Ok(QueryResult {
	// 	value: serde_json::to_value(value)?,
	// 	concerns,
	// })

	log::info!("completed affiliation metric");

	Ok(count)
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
		match CONFIG
			.get()
			.ok_or_else(|| {
				log::error!("tried to access config before set by Hipcheck core!");
				Error::UnspecifiedQueryState
			})?
			.weeks
		{
			Some(weeks) => Ok(format!("lte $ P{}w", weeks)),
			None => Ok("".to_owned()),
		}
	}

	fn explain_default_query(&self) -> Result<Option<String>> {
		Ok(Some(
			"Number of weeks since last activity in repo".to_string(),
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

// #[cfg(test)]
// mod test {
// 	use super::*;

// 	use hipcheck_sdk::types::LocalGitRepo;
// 	use jiff::{Span, SpanRound, Unit};
// 	use std::result::Result as StdResult;

// 	fn repo() -> LocalGitRepo {
// 		LocalGitRepo {
// 			path: "/home/users/me/.cache/hipcheck/clones/github/expressjs/express/".to_string(),
// 			git_ref: "main".to_string(),
// 		}
// 	}

// 	fn mock_responses() -> StdResult<MockResponses, Error> {
// 		let repo = repo();
// 		let output = "2024-06-19T19:22:45Z".to_string();

// 		// when calling into query, the input repo gets passed to `last_commit_date`, lets assume it returns the datetime `output`
// 		Ok(MockResponses::new().insert("mitre/git/last_commit_date", repo, Ok(output))?)
// 	}

// 	#[tokio::test]
// 	async fn test_activity() {
// 		let repo = repo();
// 		let target = Target {
// 			specifier: "expressjs".to_string(),
// 			local: repo,
// 			remote: None,
// 			package: None,
// 		};

// 		let mut engine = PluginEngine::mock(mock_responses().unwrap());
// 		let output = activity(&mut engine, target).await.unwrap();
// 		let span: Span = output.parse().unwrap();
// 		let result = span.round(SpanRound::new().smallest(Unit::Day)).unwrap();

// 		let today = Timestamp::now();
// 		let last_commit: Timestamp = "2024-06-19T19:22:45Z".parse().unwrap();
// 		let expected = today
// 			.since(last_commit)
// 			.unwrap()
// 			.round(SpanRound::new().smallest(Unit::Day))
// 			.unwrap();

// 		assert_eq!(result, expected);
// 	}
// }
