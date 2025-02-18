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
	collections::HashMap,
	fmt::{self, Display, Formatter},
	path::PathBuf,
	result::Result as StdResult,
	sync::OnceLock,
};

pub static ORGSSPEC: OnceLock<OrgSpec> = OnceLock::new();

#[derive(Debug, Deserialize)]
struct Config {
	orgs_spec: OrgSpec,
	// Maximum number of concerningly affilaited contributors permitted in a default query
	count_threshold: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct RawConfig {
	#[serde(rename = "orgs-file")]
	orgs_file_path: Option<String>,
	#[serde(rename = "count-threshold")]
	count_threshold: Option<u64>,
}

impl TryFrom<RawConfig> for Config {
	type Error = ConfigError;
	fn try_from(value: RawConfig) -> StdResult<Config, ConfigError> {
		if let Some(ofv) = value.orgs_file_path {
			// Get the Orgs file path and confirm it exists
			let orgs_file = PathBuf::from(&ofv);
			file::exists(&orgs_file).map_err(|_e| ConfigError::FileNotFound {
				file_path: ofv.clone(),
			})?;
			// Parse the Orgs file and construct an OrgSpec.
			let orgs_spec =
				OrgSpec::load_from(&orgs_file).map_err(|e| ConfigError::ParseError {
					source: format!("OrgSpec file at {}", ofv),
					// Print error with Debug for full context
					message: format!("{:?}", e),
				})?;
			Ok(Config {
				orgs_spec,
				count_threshold: value.count_threshold,
			})
		} else {
			Err(ConfigError::MissingRequiredConfig {
				field_name: "orgs-file".to_owned(),
				field_type: "string".to_owned(),
				possible_values: vec![],
			})
		}
	}
}

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

impl CommitContributorView {
	/// get the author of the commit
	pub fn author(&self) -> &Contributor {
		&self.author
	}

	/// get the committer of the commit
	pub fn committer(&self) -> &Contributor {
		&self.committer
	}
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

/// struct for counting number of contributions made by each contributor in the repo
struct ContributorFrequencyMap<'a>(HashMap<&'a Contributor, u64>);

impl<'a> ContributorFrequencyMap<'a> {
	fn new(capacity: Option<usize>) -> Self {
		match capacity {
			Some(capacity) => Self(HashMap::with_capacity(capacity)),
			None => Self(HashMap::new()),
		}
	}

	/// Add an affiliated_contributor to the map, update frequency for affiliated_contributor
	/// appropriately
	///
	/// If the contributor is not in the HashMap, then insert and set frequency to 1
	/// Otherwise, increment frequency by 1
	fn insert(&mut self, affiliated_contributor: &'a Contributor) {
		self.0
			.entry(affiliated_contributor)
			.and_modify(|count| *count += 1)
			.or_insert(1);
	}

	/// Retrieve a non-mutable handle to the internal HashMap
	fn inner(&self) -> &HashMap<&'a Contributor, u64> {
		&self.0
	}
}

/// Returns a boolean list with one entry per contributor to the repo
/// A `true` entry corresponds to an affiliated contributor
#[query(default)]
async fn affiliation(engine: &mut PluginEngine, key: Target) -> Result<Vec<bool>> {
	log::debug!("running affiliation query");

	// Get the OrgSpec.
	let org_spec = &ORGSSPEC.get().ok_or_else(|| {
		log::error!("tried to access config before set by Hipcheck core!");
		Error::UnspecifiedQueryState
	})?;

	// Get the commits for the source.
	let repo = key.local;

	// query the git plugin for a summary of all git contributors in the repo
	let contributors_value = engine.query("mitre/git/contributor_summary", repo).await?;
	let contributors: Vec<CommitContributorView> = serde_json::from_value(contributors_value)
		.map_err(|e| {
			log::error!("Error parsing output from mitre/git/contributor_summary: {e}");
			Error::UnexpectedPluginQueryInputFormat
		})?;

	// Use the OrgSpec to build an Affiliator.
	let affiliator = Affiliator::from_spec(org_spec).map_err(|e| {
		log::error!("failed to build affiliation checker from org spec: {}", e);
		Error::UnspecifiedQueryState
	})?;

	// attempt to reduce allocations by pre-allocating room for 500 unique contributors
	let mut contributor_freq_map = ContributorFrequencyMap::new(Some(500));
	contributors.iter().for_each(|contributor| {
		contributor_freq_map.insert(contributor.author());
		// prevent double counting a person if they are both the author and committer of a commit
		if contributor.author() != contributor.committer() {
			contributor_freq_map.insert(contributor.committer());
		}
	});

	// by default initialize to false, only need to update if a contributor is affiliated
	let mut affiliations = vec![false; contributor_freq_map.inner().keys().len()];
	for (idx, (contributor, count)) in contributor_freq_map.inner().iter().enumerate() {
		if affiliator.is_match(contributor.email.as_str()) {
			let concern = format!(
				"Contributor {} ({}) has count {}",
				contributor.name, contributor.email, count
			);
			engine.record_concern(concern);
			// SAFETY: affiliations has the same length as contributor_freq_map
			*affiliations.get_mut(idx).unwrap() = true;
		}
	}
	log::info!("completed affiliation metric");
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
		let conf: Config = serde_json::from_value::<RawConfig>(config)
			.map_err(|e| ConfigError::Unspecified {
				message: e.to_string(),
			})?
			.try_into()?;

		// Store the policy conf to be accessed only in the `default_policy_expr()` impl
		self.policy_conf
			.set(conf.count_threshold)
			.map_err(|_| ConfigError::InternalError {
				message: "plugin was already configured".to_string(),
			})?;

		ORGSSPEC
			.set(conf.orgs_spec)
			.map_err(|_e| ConfigError::InternalError {
				message: "orgs spec was already set".to_owned(),
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
	PluginServer::register(AffiliationPlugin::default())
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
			"Contributor Jane Doe (jdoe@gmail.com) has count 2"
		)
	}

	#[test]
	fn test_affiliated_contributor_freq_map() {
		let mut map = ContributorFrequencyMap::new(Some(2));
		let contributor = Contributor {
			name: "John Smith".to_owned(),
			email: "john.smith@example.com".to_owned(),
		};
		map.insert(&contributor);
		assert_eq!(*map.inner().get(&contributor).unwrap(), 1);
		map.insert(&contributor);
		assert_eq!(*map.inner().get(&contributor).unwrap(), 2);
	}
}
