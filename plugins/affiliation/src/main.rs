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
			file::exists(&orgs_file).map_err(|_e| ConfigError::InvalidConfigValue {
				field_name: "orgs-file".to_owned(),
				value: ofv.clone(),
				reason: "could not find an orgs file with that name".to_owned(),
			})?;
			// Parse the Orgs file and construct an OrgSpec.
			let orgs_spec =
				OrgSpec::load_from(&orgs_file).map_err(|e| ConfigError::InvalidConfigValue {
					field_name: "orgs-file".to_owned(),
					value: ofv.clone(),
					reason: format!("Failed to load org spec: {}", e),
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

/// A locally stored git repo, with a list of additional details
/// The details will vary based on the query (e.g. a date, a committer e-mail address, a commit hash)
///
/// This struct exists for using the temproary "batch" queries until proper batching is implemented
/// TODO: Remove this struct once batching works
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct BatchGitRepo {
	/// The local repo
	local: LocalGitRepo,

	/// Optional additional information for the query
	pub details: Vec<String>,
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

// Can be hopefully removed once Submit has chunking
mod chunk {
	use super::*;

	pub const GRPC_MAX_SIZE: usize = 1024 * 1024 * 4; // 4MB
	pub const GRPC_EFFECTIVE_MAX_SIZE: usize = 3 * (GRPC_MAX_SIZE / 4); // 1024; // Minus one KB

	pub fn chunk_hashes(
		mut hashes: Vec<String>,
		max_chunk_size: usize,
	) -> Result<Vec<Vec<String>>> {
		let mut out = vec![];

		let mut made_progress = true;
		while !hashes.is_empty() && made_progress {
			made_progress = false;
			let mut curr = vec![];
			let mut remaining = max_chunk_size;

			// While we still want to steal more bytes and we have more elements of
			// `concern` to possibly steal
			while remaining > 0 && !hashes.is_empty() {
				let c_bytes = hashes.last().unwrap().bytes().len();

				if c_bytes > max_chunk_size {
					log::error!("Query cannot be chunked, there is a concern that is larger than max chunk size");
					return Err(Error::UnspecifiedQueryState);
				} else if c_bytes <= remaining {
					// steal this concern
					let concern = hashes.pop().unwrap();
					curr.push(concern);
					remaining -= c_bytes;
					made_progress = true;
				} else {
					// Unlike concern chunking, hashes are likely to all be same size, no need to
					// keep checking if we fail on one
					break;
				}
			}
			out.push(curr);
		}

		Ok(out)
	}

	#[cfg(test)]
	mod test {
		use super::*;

		#[test]
		fn test_hash_chunk() {
			let hashes: Vec<String> = vec!["1234", "1234", "1234", "1234", "1234"]
				.into_iter()
				.map(String::from)
				.collect();
			let res = chunk_hashes(hashes, 10).unwrap();
			assert_eq!(res.len(), 3);
		}
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
	let commits_value = engine
		.query("mitre/git/commits", repo.clone())
		.await
		.map_err(|e| {
			log::error!("failed to get last commits for affiliation metric: {}", e);
			Error::UnspecifiedQueryState
		})?;
	let commits: Vec<Commit> = serde_json::from_value(commits_value)
		.map_err(|_| Error::UnexpectedPluginQueryInputFormat)?;

	// Use the OrgSpec to build an Affiliator.
	let affiliator = Affiliator::from_spec(org_spec).map_err(|e| {
		log::error!("failed to build affiliation checker from org spec: {}", e);
		Error::UnspecifiedQueryState
	})?;

	// Temporary solution to retrieve afilliated commits until batching is implemented
	// TODO: Once batching works, revert to using looped calls of contributors_to_commit() in the commented out code
	// @Note - this lacks a way to chunk the commits into `contributors_for_commit`. Until this
	// code is updated ot do so or outbound PluginQuerys get a SubmitInProgress state, this will be
	// broken when analyzing large repos.

	// for commit in commits.iter() {
	// 	// Check if a commit matches the affiliation rules.
	// 	let hash = commit.hash.clone();
	// 	let detailed_repo = DetailedGitRepo {
	// 		local: repo.clone(),
	// 		details: Some(hash.clone()),
	// 	};
	// 	let view_value = engine
	// 		.query("mitre/git/contributors_for_commit", detailed_repo)
	// 		.await
	// 		.map_err(|e| {
	// 			log::error!("failed to get contributors for commit {}: {}", hash, e);
	// 			Error::UnspecifiedQueryState
	// 		})?;
	// 	let commit_view: CommitContributorView = serde_json::from_value(view_value)
	// 		.map_err(|_| Error::UnexpectedPluginQueryInputFormat)?;
	// for commit_view in commit_views {
	// Get the affiliation type for the commit
	// affiliations.push(AffiliationDetails {
	// 	commit: commit.clone(),
	// 	affiliated_type,
	// });
	// }
	// let affiliated_iter = affiliations
	// 	.into_iter()
	// 	.filter(|a| a.affiliated_type.is_affiliated())

	let mut contributors = HashSet::new();
	let mut contributor_freq_map = HashMap::new();

	// Get the hashes for each commit
	let hashes = commits.iter().map(|c| c.hash.clone()).collect();

	// Chunk hashes because for large repos the request message would be too large
	let chunked_hashes = chunk::chunk_hashes(hashes, chunk::GRPC_EFFECTIVE_MAX_SIZE)?;

	let mut commit_views: Vec<CommitContributorView> = vec![];
	for hashes in chunked_hashes {
		// Repo with the hash of every commit
		let commit_batch_repo = BatchGitRepo {
			local: repo.clone(),
			details: hashes,
		};
		// Get a list of lookup structs for linking contributors to each commit
		let commit_values = engine
			.query("mitre/git/batch_contributors_for_commit", commit_batch_repo)
			.await
			.map_err(|e| {
				log::error!("failed to get contributors for commits: {}", e);
				Error::UnspecifiedQueryState
			})?;
		let views: Vec<CommitContributorView> = serde_json::from_value(commit_values)
			.map_err(|_| Error::UnexpectedPluginQueryInputFormat)?;
		commit_views.extend(views.into_iter());
	}

	// For each commit, collect contributors that fail the affiliation rules
	for commit_view in commit_views {
		// Get the affiliation type for the commit
		let affiliated_type = AffiliatedType::is(&affiliator, &commit_view);

		// If the contributors fail the rules, add them to the contributor hash set
		if affiliated_type.is_affiliated() {
			match affiliated_type {
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
	}

	// Temporary solution to retrieve afilliated commits until batching is implemented
	// TODO: Once batching works, revert to using looped calls of commits_for_contributor() in the commented out code

	// let mut contributors = HashSet::new();
	// let mut contributor_freq_map = HashMap::new();

	// // Get the affiliated contributors from the commits
	// for affiliation in affiliated_iter {
	// 	let hash = affiliation.commit.hash;
	// 	let commit_repo = DetailedGitRepo {
	// 		local: repo.clone(),
	// 		details: Some(hash.clone()),
	// 	};
	// 	let view_value = engine
	// 		.query("mitre/git/contributors_for_commit", commit_repo)
	// 		.await
	// 		.map_err(|e| {
	// 			log::error!("failed to get contributors for commit {}: {}", hash, e);
	// 			Error::UnspecifiedQueryState
	// 		})?;
	// 	let commit_view: CommitContributorView = serde_json::from_value(view_value)
	// 		.map_err(|_| Error::UnexpectedPluginQueryInputFormat)?;

	// Get the emails for each affiliated contributor
	let emails = contributors.iter().map(|c| c.1.clone()).collect();
	// Repo with the email of every affiliated contributor
	let contributor_batch_repo = BatchGitRepo {
		local: repo.clone(),
		details: emails,
	};

	// Get a list of lookup structs for linking commits to each affiliated contributor
	let contributor_values = engine
		.query(
			"mitre/git/batch_commits_for_contributor",
			contributor_batch_repo,
		)
		.await
		.map_err(|e| {
			log::error!("failed to get commits for contributors: {}", e);
			Error::UnspecifiedQueryState
		})?;
	let contributor_views: Vec<ContributorView> = serde_json::from_value(contributor_values)
		.map_err(|_| Error::UnexpectedPluginQueryInputFormat)?;

	// For each affiliated contributor, count how many commits they contributed to,
	// then add the contributor's name and its commit count to the contributor frequency hash map
	for contributor_view in contributor_views {
		let count = contributor_view.commits.len();
		contributor_freq_map.insert(
			format!(
				"{} ({})",
				contributor_view.contributor.name, contributor_view.contributor.email
			),
			count,
		);
	}

	let all_contributors_value = engine
		.query("mitre/git/contributors", repo.clone())
		.await
		.map_err(|e| {
			log::error!("failed to get list of all contributors to repo: {}", e);
			Error::UnspecifiedQueryState
		})?;
	let all_contributors: Vec<Contributor> = serde_json::from_value(all_contributors_value)
		.map_err(|_| Error::UnexpectedPluginQueryInputFormat)?;
	let all_emails: Vec<String> = all_contributors.iter().map(|c| c.email.clone()).collect();

	let affiliated_emails: Vec<String> = contributors.iter().map(|c| c.1.clone()).collect();
	let affiliations = all_emails
		.iter()
		.map(|e| affiliated_emails.contains(e))
		.collect();

	// Add each contributor-count pair as a concern
	for (contributor, count) in contributor_freq_map.into_iter() {
		let concern = format!("Contributor {} has count {}", contributor, count);
		engine.record_concern(concern);
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
			.map_err(|_| ConfigError::Unspecified {
				message: "plugin was already configured".to_string(),
			})?;

		ORGSSPEC
			.set(conf.orgs_spec)
			.map_err(|_e| ConfigError::Unspecified {
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

		let contributor_1_view = ContributorView {
			contributor: contributor_1.clone(),
			commits: vec![commit_1.clone(), commit_2.clone()],
		};

		let contributor_2_view = ContributorView {
			contributor: contributor_2.clone(),
			commits: vec![commit_2.clone(), commit_3.clone()],
		};

		let commits_repo = BatchGitRepo {
			local: repo.clone(),
			details: vec![
				"ghi-789".to_string(),
				"def-456".to_string(),
				"abc-123".to_string(),
			],
		};

		let contributors_repo = BatchGitRepo {
			local: repo.clone(),
			details: vec!["jsmith@mitre.org".to_string(), "jdoe@gmail.com".to_string()],
		};

		let contributor_1_repo = BatchGitRepo {
			local: repo.clone(),
			details: vec!["jsmith@mitre.org".to_string()],
		};

		let contributor_2_repo = BatchGitRepo {
			local: repo.clone(),
			details: vec!["jdoe@gmail.com".to_string()],
		};

		let mut mock_responses = MockResponses::new();

		mock_responses
			.insert(
				"mitre/git/commits",
				repo.clone(),
				Ok(vec![commit_1, commit_2, commit_3]),
			)
			.unwrap();
		mock_responses
			.insert(
				"mitre/git/contributors",
				repo,
				Ok(vec![contributor_1, contributor_2]),
			)
			.unwrap();
		mock_responses
			.insert(
				"mitre/git/batch_contributors_for_commit",
				commits_repo,
				Ok(vec![commit_3_view, commit_2_view, commit_1_view]),
			)
			.unwrap();
		mock_responses
			.insert(
				"mitre/git/batch_commits_for_contributor",
				contributors_repo,
				Ok(vec![contributor_1_view.clone(), contributor_2_view.clone()]),
			)
			.unwrap();
		mock_responses
			.insert(
				"mitre/git/batch_commits_for_contributor",
				contributor_1_repo,
				Ok(vec![contributor_1_view]),
			)
			.unwrap();
		mock_responses
			.insert(
				"mitre/git/batch_commits_for_contributor",
				contributor_2_repo,
				Ok(vec![contributor_2_view]),
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

		let concerns = engine.take_concerns();

		assert_eq!(output.len(), 2);
		let num_affiliated = output.iter().filter(|&n| *n).count();
		assert_eq!(num_affiliated, 1);
		assert_eq!(
			concerns[0],
			"Contributor Jane Doe (jdoe@gmail.com) has count 2"
		)
	}
}
