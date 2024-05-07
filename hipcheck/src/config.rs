// SPDX-License-Identifier: Apache-2.0

//! Defines the configuration file format.

use crate::context::Context;
use crate::error::Result;
use crate::filesystem as file;
use crate::BINARY_CONFIG_FILE;
use crate::F64;
use crate::LANGS_FILE;
use crate::ORGS_FILE;
use crate::TYPO_FILE;
use pathbuf::pathbuf;
use serde::Deserialize;
use serde::Serialize;
use smart_default::SmartDefault;
use std::default::Default;
use std::path::Path;
use std::path::PathBuf;
use std::rc::Rc;

impl Config {
	/// Load configuration from the given directory.
	pub fn load_from(config_path: &Path) -> Result<Config> {
		file::exists(config_path)?;
		let config = file::read_toml(config_path).context("can't parse config file")?;

		Ok(config)
	}
}

/// Represents the configuration of Hipcheck's analyses.
#[derive(Debug, Deserialize, Serialize, Default, PartialEq, Eq)]
#[serde(default)]
pub struct Config {
	/// The configuration of overall project risk tolerance.
	#[serde(default)]
	pub risk: RiskConfig,

	/// The configuration of Hipcheck's different analyses.
	#[serde(default)]
	pub analysis: AnalysisConfig,

	/// The configuration of Hipcheck's knowledge about languages.
	#[serde(default)]
	pub languages: LanguagesConfig,
}

/// Represents configuration of the overall risk threshold of an assessment.
#[derive(Debug, Serialize, Deserialize, SmartDefault, PartialEq, Eq)]
#[serde(default)]
pub struct RiskConfig {
	/// The risk tolerance threshold, a value between 0 and 1.
	#[default(_code = "F64::new(0.5).unwrap()")]
	#[serde(deserialize_with = "de::percent")]
	pub threshold: F64,
}

/// Defines configuration for all of Hipcheck's analyses.
#[derive(Debug, Deserialize, Serialize, Default, PartialEq, Eq)]
#[serde(default)]
pub struct AnalysisConfig {
	/// Defines configuration for practices analysis.
	#[serde(default)]
	pub practices: PracticesConfig,

	/// Defines configuration for attack analysis.
	#[serde(default)]
	pub attacks: AttacksConfig,
}

/// Configuration of analyses on a repo's development practices.
#[derive(Debug, Deserialize, Serialize, SmartDefault, PartialEq, Eq)]
#[serde(default)]
pub struct PracticesConfig {
	/// Whether the analysis is active.
	#[default = true]
	pub active: bool,

	/// How heavily the analysis' results weigh in risk scoring.
	#[default = 1]
	pub weight: u64,

	/// Defines configuration for activity analysis.
	#[serde(default)]
	pub activity: ActivityConfig,

	/// Defines configuration for binary file analysis.
	#[serde(default)]
	pub binary: BinaryConfig,

	/// Defines configuration for in fuzz analysis.
	#[serde(default)]
	pub fuzz: FuzzConfig,

	/// Defines configuration for identity analysis.
	#[serde(default)]
	pub identity: IdentityConfig,

	/// Defines configuration for review analysis.
	#[serde(default)]
	pub review: ReviewConfig,
}

/// Configuration of analyses on potential attacks against a repo.
#[derive(Debug, Deserialize, Serialize, SmartDefault, PartialEq, Eq)]
#[serde(default)]
pub struct AttacksConfig {
	/// Whether the analysis is active.
	#[default = true]
	pub active: bool,

	/// How heavily the analysis' results weigh in risk scoring.
	#[default = 1]
	pub weight: u64,

	/// Defines configuration for typo analysis.
	#[serde(default)]
	pub typo: TypoConfig,

	/// Defines configuration for commit analysis.
	#[serde(default)]
	pub commit: CommitConfig,
}

/// Configuration of analyses on individual commits.
#[derive(Debug, Deserialize, Serialize, SmartDefault, PartialEq, Eq)]
#[serde(default)]
pub struct CommitConfig {
	/// Whether the analysis is active.
	#[default = true]
	pub active: bool,

	/// How heavily the analysis' results weigh in risk scoring.
	#[default = 1]
	pub weight: u64,

	/// Defines configuration for affiliation analysis.
	#[serde(default)]
	pub affiliation: AffiliationConfig,

	/// Defines configuration for churn analysis.
	#[serde(default)]
	pub churn: ChurnConfig,

	/// Defines configuration for contributor trust analysis.
	#[serde(default)]
	pub contributor_trust: ContributorTrustConfig,

	/// Defines configuration for contributor trust analysis.
	#[serde(default)]
	pub commit_trust: CommitTrustConfig,

	/// Defines configuration for entropy analysis.
	#[serde(default)]
	pub entropy: EntropyConfig,

	/// Defines configuration for pull request affiliation analysis.
	#[serde(default)]
	pub pr_affiliation: PrAffiliationConfig,

	/// Defines configuration for pull request module contributors analysis.
	#[serde(default)]
	pub pr_module_contributors: PrModuleContributorsConfig,
}

/// Defines configuration for activity analysis.
#[derive(Debug, Deserialize, Serialize, SmartDefault, PartialEq, Eq)]
#[serde(default)]
pub struct ActivityConfig {
	/// Whether the analysis is active.
	#[default = true]
	pub active: bool,

	/// How heavily the analysis' results weigh in risk scoring.
	#[default = 1]
	pub weight: u64,

	/// A number of weeks, over which a repo fails the analysis.
	#[default = 71]
	pub week_count_threshold: u64,
}

/// Defines configuration for affiliation analysis.
#[derive(Debug, Deserialize, Serialize, SmartDefault, PartialEq, Eq)]
#[serde(default)]
pub struct AffiliationConfig {
	/// Whether the analysis is active.
	#[default = true]
	pub active: bool,

	/// How heavily the analysis' results weigh in risk scoring.
	#[default = 1]
	pub weight: u64,

	/// A number of affiliations permitted, over which a repo fails the analysis.
	#[default = 0]
	pub count_threshold: u64,

	/// An "orgs file" containing info for affiliation matching.
	#[default = "Orgs.toml"]
	pub orgs_file: String,
}

/// Defines configuration for binary file analysis.
#[derive(Debug, Deserialize, Serialize, SmartDefault, PartialEq, Eq)]
#[serde(default)]
pub struct BinaryConfig {
	/// Binary file extension configuration file.
	#[default = "Binary.toml"]
	pub binary_config_file: String,

	/// Whether the analysis is active.
	#[default = true]
	pub active: bool,

	/// How heavily the analysis' results weigh in risk scoring.
	#[default = 1]
	pub weight: u64,

	/// A count of binary files over which a repo fails the analysis.
	#[default = 0]
	pub binary_file_threshold: u64,
}

/// Defines configuration for churn analysis.
#[derive(Debug, Deserialize, Serialize, SmartDefault, PartialEq, Eq)]
#[serde(default)]
pub struct ChurnConfig {
	/// Whether the analysis is active.
	#[default = true]
	pub active: bool,

	/// How heavily the analysis' results weigh in risk scoring.
	#[default = 1]
	pub weight: u64,

	/// A churn Z-score, over which a commit is marked as "bad"
	#[default(_code = "F64::new(3.0).unwrap()")]
	pub value_threshold: F64,

	/// A percentage of "bad" commits over which a repo fails the analysis.
	#[default(_code = "F64::new(0.02).unwrap()")]
	#[serde(deserialize_with = "de::percent")]
	pub percent_threshold: F64,
}

/// Defines configuration for commit trust analysis.
#[derive(Debug, Deserialize, Serialize, SmartDefault, PartialEq, Eq)]
#[serde(default)]
pub struct CommitTrustConfig {
	/// Whether the analysis is active.
	#[default = true]
	pub active: bool,

	/// How heavily the analysis' results weigh in risk scoring.
	#[default = 1]
	pub weight: u64,
}

/// Defines configuration for contributor trust analysis.
#[derive(Debug, Deserialize, Serialize, SmartDefault, PartialEq, Eq)]
#[serde(default)]
pub struct ContributorTrustConfig {
	/// Whether the analysis is active.
	#[default = true]
	pub active: bool,

	/// How heavily the analysis' results weigh in risk scoring.
	#[default = 1]
	pub weight: u64,

	/// A trust N-score, number of commits over which a commitor is marked as trusted or not
	#[default = 3]
	pub value_threshold: u64,

	/// A number of months over which a contributor would be tracked for trust.
	#[default = 3]
	pub trust_month_count_threshold: u64,

	/// A percentage of "bad" commits over which a repo fails the analysis because commit is not trusted.
	#[default(_code = "F64::new(0.0).unwrap()")]
	#[serde(deserialize_with = "de::percent")]
	pub percent_threshold: F64,
}

/// Defines configuration for entropy analysis.
#[derive(Debug, Deserialize, Serialize, SmartDefault, PartialEq, Eq)]
#[serde(default)]
pub struct EntropyConfig {
	/// Whether the analysis is active.
	#[default = true]
	pub active: bool,

	/// How heavily the analysis' results weigh in risk scoring.
	#[default = 1]
	pub weight: u64,

	/// An entropy Z-score, over which a commit is marked as "bad"
	#[default(_code = "F64::new(10.0).unwrap()")]
	pub value_threshold: F64,

	/// A percentage of "bad" commits over which a repo fails the analysis.
	#[default(_code = "F64::new(0.0).unwrap()")]
	#[serde(deserialize_with = "de::percent")]
	pub percent_threshold: F64,
}

/// Defines configuration for identity analysis.
#[derive(Debug, Deserialize, Serialize, SmartDefault, PartialEq, Eq)]
#[serde(default)]
pub struct IdentityConfig {
	/// Whether the analysis is active.
	#[default = true]
	pub active: bool,

	/// How heavily the analysis' results weigh in risk scoring.
	#[default = 1]
	pub weight: u64,

	/// A percentage of commits permitted to have a mismatch between committer and
	/// submitter identity, over which a repo fails the analysis.
	#[default(_code = "F64::new(0.20).unwrap()")]
	#[serde(deserialize_with = "de::percent")]
	pub percent_threshold: F64,
}

/// Defines configuration for review analysis.
#[derive(Debug, Deserialize, Serialize, SmartDefault, PartialEq, Eq)]
#[serde(default)]
pub struct ReviewConfig {
	/// Whether the analysis is active.
	#[default = true]
	pub active: bool,

	/// How heavily the analysis' results weigh in risk scoring.
	#[default = 1]
	pub weight: u64,

	/// A percentage of pull requests permitted to not have review prior to being
	/// merged, over which a repo fails the analysis.
	#[default(_code = "F64::new(0.05).unwrap()")]
	#[serde(deserialize_with = "de::percent")]
	pub percent_threshold: F64,
}

/// Defines configuration for typo analysis.
#[derive(Debug, Deserialize, Serialize, SmartDefault, PartialEq, Eq)]
#[serde(default)]
pub struct TypoConfig {
	/// Whether the analysis is active.
	#[default = true]
	pub active: bool,

	/// How heavily the analysis' results weigh in risk scoring.
	#[default = 1]
	pub weight: u64,

	/// The number of potential dependency name typos permitted, over which
	/// a repo fails the analysis.
	#[default = 0]
	pub count_threshold: u64,

	/// Path to a "typos file" containing necessary information for typo detection.
	#[default = "Typos.toml"]
	pub typo_file: String,
}

/// Defines configuration for pull request affiliation analysis.
#[derive(Debug, Deserialize, Serialize, SmartDefault, PartialEq, Eq)]
#[serde(default)]
pub struct PrAffiliationConfig {
	/// Whether the analysis is active.
	#[default = true]
	pub active: bool,

	/// How heavily the analysis' results weigh in risk scoring.
	#[default = 1]
	pub weight: u64,

	/// A number of affiliations permitted, over which a repo fails the analysis.
	#[default = 0]
	pub count_threshold: u64,
}

/// Defines configuration for pull request module committers analysis.
#[derive(Debug, Deserialize, Serialize, SmartDefault, PartialEq, Eq)]
#[serde(default)]
pub struct PrModuleContributorsConfig {
	/// Whether the analysis is active.
	#[default = true]
	pub active: bool,

	/// How heavily the analysis' results weigh in risk scoring.
	#[default = 1]
	pub weight: u64,

	/// Percent of committers working on a module for the first time permitted, over which a repo fails the analysis.
	#[default(_code = "F64::new(0.30).unwrap()")]
	#[serde(deserialize_with = "de::percent")]
	pub percent_threshold: F64,
}

/// Defines the configuration of language-specific info.
#[derive(Debug, Deserialize, Serialize, SmartDefault, PartialEq, Eq)]
#[serde(default)]
pub struct LanguagesConfig {
	/// The file to pull language information from.
	#[default = "Langs.toml"]
	pub langs_file: String,
}

/// Defines configuration for fuzz analysis.
#[derive(Debug, Deserialize, Serialize, SmartDefault, PartialEq, Eq)]
#[serde(default)]
pub struct FuzzConfig {
	/// Whether the analysis is active.
	#[default = true]
	pub active: bool,

	/// How heavily the analysis' results weigh in risk scoring.
	#[default = 1]
	pub weight: u64,
}

/// Inner module for deserialization helpers.
mod de {
	use super::F64;
	use serde::de;
	use serde::de::Deserializer;
	use serde::de::Visitor;
	use std::fmt;
	use std::fmt::Formatter;

	/// Deserialize a float, ensuring it's between 0.0 and 1.0 inclusive.
	pub(super) fn percent<'de, D>(deserializer: D) -> Result<F64, D::Error>
	where
		D: Deserializer<'de>,
	{
		struct PercentVisitor;

		impl<'de> Visitor<'de> for PercentVisitor {
			type Value = f64;

			fn expecting(&self, formatter: &mut Formatter) -> fmt::Result {
				formatter.write_str("a float between 0.0 and 1.0 inclusive")
			}

			fn visit_f64<E>(self, value: f64) -> Result<f64, E>
			where
				E: de::Error,
			{
				if is_percent(value) {
					Ok(value)
				} else {
					Err(de::Error::custom("must be between 0.0 and 1.0 inclusive"))
				}
			}
		}

		// Deserialize and return as `F64`
		let percent = deserializer.deserialize_f64(PercentVisitor)?;
		Ok(F64::new(percent).unwrap())
	}

	/// Check if a float is a valid percent value.
	fn is_percent(f: f64) -> bool {
		(0.0..=1.0).contains(&f)
	}
}

/// Query for accessing a source of Hipcheck config data
#[salsa::query_group(ConfigSourceStorage)]
pub trait ConfigSource: salsa::Database {
	/// Returns the input `Config` struct
	#[salsa::input]
	fn config(&self) -> Rc<Config>;
	/// Returns the directory containing the config file
	#[salsa::input]
	fn config_dir(&self) -> Rc<PathBuf>;
	/// Returns the token set in HC_GITHUB_TOKEN env var
	#[salsa::input]
	fn github_api_token(&self) -> Option<Rc<String>>;
}

/// Query for accessing the risk threshold config
#[salsa::query_group(RiskConfigQueryStorage)]
pub trait RiskConfigQuery: ConfigSource {
	/// Returns the risk threshold
	fn risk_threshold(&self) -> F64;
}

/// Query for accessing the languages analysis config
#[salsa::query_group(LanguagesConfigQueryStorage)]
pub trait LanguagesConfigQuery: ConfigSource {
	/// Returns the langs file path relative to the config file
	fn langs_file_rel(&self) -> Rc<String>;
	/// Returns the langs file absolute path
	fn langs_file(&self) -> Rc<PathBuf>;
}

/// Queries for accessing the fuzz analysis config
#[salsa::query_group(FuzzConfigQueryStorage)]
pub trait FuzzConfigQuery: ConfigSource {
	/// Returns the fuzz analysis active status
	fn fuzz_active(&self) -> bool;
	/// Returns the fuzz analysis weight
	fn fuzz_weight(&self) -> u64;
}

/// Queries for accessing the practices analysis config
#[salsa::query_group(PracticesConfigQueryStorage)]
pub trait PracticesConfigQuery: ConfigSource {
	/// Returns the practices analysis active status
	fn practices_active(&self) -> bool;
	/// Returns the practices analysis weight
	fn practices_weight(&self) -> u64;

	/// Returns the activity analysis active status
	fn activity_active(&self) -> bool;
	/// Returns the activity analysis weight
	fn activity_weight(&self) -> u64;
	/// Returns the activity analysis week-count threshold
	fn activity_week_count_threshold(&self) -> u64;

	/// Returns the binary file analysis active status
	fn binary_active(&self) -> bool;
	/// Returns the binary file analysis weight
	fn binary_weight(&self) -> u64;
	/// Returns the binary file analysis count threshold
	fn binary_count_threshold(&self) -> u64;
	/// Returns the binary formats file path relative to the
	/// config file
	fn binary_formats_file_rel(&self) -> Rc<String>;
	/// Returns the binary formats file absolute path
	fn binary_formats_file(&self) -> Rc<PathBuf>;

	/// Returns the identity analysis active status
	fn identity_active(&self) -> bool;
	/// Returns the identity analysis weight
	fn identity_weight(&self) -> u64;
	/// Returns the identity analysis percent threshold
	fn identity_percent_threshold(&self) -> F64;

	/// Returns the review analysis active status
	fn review_active(&self) -> bool;
	/// Returns the review analysis weight
	fn review_weight(&self) -> u64;
	/// Returns the review analysis percent threshold
	fn review_percent_threshold(&self) -> F64;
}

/// Queries for accessing the attacks analysis config
#[salsa::query_group(AttacksConfigQueryStorage)]
pub trait AttacksConfigQuery: CommitConfigQuery {
	/// Returns the attacks analysis active status
	fn attacks_active(&self) -> bool;
	/// Returns the attacks analysis weight
	fn attacks_weight(&self) -> u64;

	/// Returns the typo analysis active status
	fn typo_active(&self) -> bool;
	/// Returns the typo analysis weight
	fn typo_weight(&self) -> u64;
	/// Returns the typo analysis count threshold
	fn typo_count_threshold(&self) -> u64;
	/// Returns the typo file path relative to the config file
	fn typo_file_rel(&self) -> Rc<String>;
	/// Returns the typo file absolute path
	fn typo_file(&self) -> Rc<PathBuf>;
}

/// Queries for accessing the commit analysis config
#[salsa::query_group(CommitConfigQueryStorage)]
pub trait CommitConfigQuery: ConfigSource {
	/// Returns the commit analysis active status
	fn commit_active(&self) -> bool;
	/// Returns the commit analysis weight
	fn commit_weight(&self) -> u64;

	/// Returns the affiliation analysis active status
	fn affiliation_active(&self) -> bool;
	/// Returns the affiliation analysis weight
	fn affiliation_weight(&self) -> u64;
	/// Returns the affiliation analysis count threshold
	fn affiliation_count_threshold(&self) -> u64;
	/// Returns the orgs file path relative to the config file
	fn orgs_file_rel(&self) -> Rc<String>;
	/// Returns the orgs file absolute path
	fn orgs_file(&self) -> Rc<PathBuf>;

	/// Returns the churn analysis active status
	fn churn_active(&self) -> bool;
	/// Returns the churn analysis weight
	fn churn_weight(&self) -> u64;
	/// Returns the churn analysis count threshold
	fn churn_value_threshold(&self) -> F64;
	/// Returns the churn analysis percent threshold
	fn churn_percent_threshold(&self) -> F64;

	/// Returns the commit trust analysis active status
	fn commit_trust_active(&self) -> bool;
	/// Returns the commit trust analysis weight
	fn commit_trust_weight(&self) -> u64;

	/// Returns the contributor trust analysis active status
	fn contributor_trust_active(&self) -> bool;
	/// Returns the contributor trust analysis weight
	fn contributor_trust_weight(&self) -> u64;
	/// Returns the contributor trust analysis count threshold
	fn contributor_trust_value_threshold(&self) -> u64;
	/// Returns the contributor trust analysis month threshold
	fn contributor_trust_month_count_threshold(&self) -> u64;
	/// Returns the contributor trust analysis percent threshold
	fn contributor_trust_percent_threshold(&self) -> F64;

	/// Returns the entropy analysis active status
	fn entropy_active(&self) -> bool;
	/// Returns the entropy analysis weight
	fn entropy_weight(&self) -> u64;
	/// Returns the entropy analysis value threshold
	fn entropy_value_threshold(&self) -> F64;
	/// Returns the entropy analysis percent threshold
	fn entropy_percent_threshold(&self) -> F64;

	/// Returns the pull request affiliation analysis active status
	fn pr_affiliation_active(&self) -> bool;
	/// Returns the pull request affiliation analysis weight
	fn pr_affiliation_weight(&self) -> u64;
	/// Returns the pull request affiliation analysis count threshold
	fn pr_affiliation_count_threshold(&self) -> u64;
	// Pull request affiliation resues orgs_file functions from repo affiliation

	/// Returns the pull request module contributors analysis active status
	fn pr_module_contributors_active(&self) -> bool;
	/// Returns the pull request module contributors analysis weight
	fn pr_module_contributors_weight(&self) -> u64;
	/// Returns the pull request module contributors analysis count threshold
	fn pr_module_contributors_percent_threshold(&self) -> F64;
}

/// Derived query implementations

/// In general, these simply return the value of a particular field in
/// one of the `Config` child structs.  When the type of the desired
/// field is `String`, it is returned wrapped in an `Rc`.  This is
/// done to keep Salsa's cloning cheap.

fn risk_threshold(db: &dyn RiskConfigQuery) -> F64 {
	let config = db.config();
	config.risk.threshold
}

fn langs_file_rel(_db: &dyn LanguagesConfigQuery) -> Rc<String> {
	Rc::new(LANGS_FILE.to_string())
}

fn langs_file(db: &dyn LanguagesConfigQuery) -> Rc<PathBuf> {
	Rc::new(pathbuf![
		db.config_dir().as_ref(),
		db.langs_file_rel().as_ref()
	])
}

fn fuzz_active(db: &dyn FuzzConfigQuery) -> bool {
	let config = db.config();
	config.analysis.practices.fuzz.active
}

fn fuzz_weight(db: &dyn FuzzConfigQuery) -> u64 {
	let config = db.config();
	config.analysis.practices.fuzz.weight
}

fn practices_active(db: &dyn PracticesConfigQuery) -> bool {
	let config = db.config();
	config.analysis.practices.active
}

fn practices_weight(db: &dyn PracticesConfigQuery) -> u64 {
	let config = db.config();
	config.analysis.practices.weight
}

fn activity_active(db: &dyn PracticesConfigQuery) -> bool {
	let config = db.config();
	config.analysis.practices.activity.active
}

fn activity_weight(db: &dyn PracticesConfigQuery) -> u64 {
	let config = db.config();
	config.analysis.practices.activity.weight
}

fn activity_week_count_threshold(db: &dyn PracticesConfigQuery) -> u64 {
	let config = db.config();
	config.analysis.practices.activity.week_count_threshold
}

fn binary_active(db: &dyn PracticesConfigQuery) -> bool {
	let config = db.config();
	config.analysis.practices.binary.active
}

fn binary_weight(db: &dyn PracticesConfigQuery) -> u64 {
	let config = db.config();
	config.analysis.practices.binary.weight
}

fn binary_count_threshold(db: &dyn PracticesConfigQuery) -> u64 {
	let config = db.config();
	config.analysis.practices.binary.binary_file_threshold
}

fn binary_formats_file_rel(_db: &dyn PracticesConfigQuery) -> Rc<String> {
	Rc::new(BINARY_CONFIG_FILE.to_string())
}

fn binary_formats_file(db: &dyn PracticesConfigQuery) -> Rc<PathBuf> {
	Rc::new(pathbuf![
		db.config_dir().as_ref(),
		db.binary_formats_file_rel().as_ref()
	])
}

fn identity_active(db: &dyn PracticesConfigQuery) -> bool {
	let config = db.config();
	config.analysis.practices.identity.active
}

fn identity_weight(db: &dyn PracticesConfigQuery) -> u64 {
	let config = db.config();
	config.analysis.practices.identity.weight
}

fn identity_percent_threshold(db: &dyn PracticesConfigQuery) -> F64 {
	let config = db.config();
	config.analysis.practices.identity.percent_threshold
}

fn review_active(db: &dyn PracticesConfigQuery) -> bool {
	let config = db.config();
	config.analysis.practices.review.active
}

fn review_weight(db: &dyn PracticesConfigQuery) -> u64 {
	let config = db.config();
	config.analysis.practices.review.weight
}

fn review_percent_threshold(db: &dyn PracticesConfigQuery) -> F64 {
	let config = db.config();
	config.analysis.practices.review.percent_threshold
}

fn attacks_active(db: &dyn AttacksConfigQuery) -> bool {
	let config = db.config();
	config.analysis.attacks.active
}

fn attacks_weight(db: &dyn AttacksConfigQuery) -> u64 {
	let config = db.config();
	config.analysis.attacks.weight
}

fn typo_active(db: &dyn AttacksConfigQuery) -> bool {
	let config = db.config();
	config.analysis.attacks.typo.active
}

fn typo_weight(db: &dyn AttacksConfigQuery) -> u64 {
	let config = db.config();
	config.analysis.attacks.typo.weight
}

fn typo_count_threshold(db: &dyn AttacksConfigQuery) -> u64 {
	let config = db.config();
	config.analysis.attacks.typo.count_threshold
}

fn typo_file_rel(_db: &dyn AttacksConfigQuery) -> Rc<String> {
	Rc::new(TYPO_FILE.to_string())
}

fn typo_file(db: &dyn AttacksConfigQuery) -> Rc<PathBuf> {
	Rc::new(pathbuf![
		db.config_dir().as_ref(),
		db.typo_file_rel().as_ref()
	])
}

fn commit_active(db: &dyn CommitConfigQuery) -> bool {
	let config = db.config();
	config.analysis.attacks.commit.active
}

fn commit_weight(db: &dyn CommitConfigQuery) -> u64 {
	let config = db.config();
	config.analysis.attacks.commit.weight
}

fn affiliation_active(db: &dyn CommitConfigQuery) -> bool {
	let config = db.config();
	config.analysis.attacks.commit.affiliation.active
}

fn affiliation_weight(db: &dyn CommitConfigQuery) -> u64 {
	let config = db.config();
	config.analysis.attacks.commit.affiliation.weight
}

fn affiliation_count_threshold(db: &dyn CommitConfigQuery) -> u64 {
	let config = db.config();
	config.analysis.attacks.commit.affiliation.count_threshold
}

fn orgs_file_rel(_db: &dyn CommitConfigQuery) -> Rc<String> {
	Rc::new(ORGS_FILE.to_string())
}

fn orgs_file(db: &dyn CommitConfigQuery) -> Rc<PathBuf> {
	Rc::new(pathbuf![
		db.config_dir().as_ref(),
		db.orgs_file_rel().as_ref()
	])
}

fn churn_active(db: &dyn CommitConfigQuery) -> bool {
	let config = db.config();
	config.analysis.attacks.commit.churn.active
}

fn churn_weight(db: &dyn CommitConfigQuery) -> u64 {
	let config = db.config();
	config.analysis.attacks.commit.churn.weight
}

fn churn_value_threshold(db: &dyn CommitConfigQuery) -> F64 {
	let config = db.config();
	config.analysis.attacks.commit.churn.value_threshold
}

fn churn_percent_threshold(db: &dyn CommitConfigQuery) -> F64 {
	let config = db.config();
	config.analysis.attacks.commit.churn.percent_threshold
}

fn contributor_trust_active(db: &dyn CommitConfigQuery) -> bool {
	let config = db.config();
	config.analysis.attacks.commit.contributor_trust.active
}

fn contributor_trust_weight(db: &dyn CommitConfigQuery) -> u64 {
	let config = db.config();
	config.analysis.attacks.commit.contributor_trust.weight
}

fn contributor_trust_value_threshold(db: &dyn CommitConfigQuery) -> u64 {
	let config = db.config();
	config
		.analysis
		.attacks
		.commit
		.contributor_trust
		.value_threshold
}

fn contributor_trust_month_count_threshold(db: &dyn CommitConfigQuery) -> u64 {
	let config = db.config();
	config
		.analysis
		.attacks
		.commit
		.contributor_trust
		.trust_month_count_threshold
}

fn contributor_trust_percent_threshold(db: &dyn CommitConfigQuery) -> F64 {
	let config = db.config();
	config
		.analysis
		.attacks
		.commit
		.contributor_trust
		.percent_threshold
}

fn commit_trust_active(db: &dyn CommitConfigQuery) -> bool {
	let config = db.config();
	config.analysis.attacks.commit.commit_trust.active
}

fn commit_trust_weight(db: &dyn CommitConfigQuery) -> u64 {
	let config = db.config();
	config.analysis.attacks.commit.commit_trust.weight
}

fn entropy_active(db: &dyn CommitConfigQuery) -> bool {
	let config = db.config();
	config.analysis.attacks.commit.entropy.active
}

fn entropy_weight(db: &dyn CommitConfigQuery) -> u64 {
	let config = db.config();
	config.analysis.attacks.commit.entropy.weight
}

fn entropy_value_threshold(db: &dyn CommitConfigQuery) -> F64 {
	let config = db.config();
	config.analysis.attacks.commit.entropy.value_threshold
}

fn entropy_percent_threshold(db: &dyn CommitConfigQuery) -> F64 {
	let config = db.config();
	config.analysis.attacks.commit.entropy.percent_threshold
}

fn pr_affiliation_active(db: &dyn CommitConfigQuery) -> bool {
	let config = db.config();
	config.analysis.attacks.commit.pr_affiliation.active
}

fn pr_affiliation_weight(db: &dyn CommitConfigQuery) -> u64 {
	let config = db.config();
	config.analysis.attacks.commit.pr_affiliation.weight
}

fn pr_affiliation_count_threshold(db: &dyn CommitConfigQuery) -> u64 {
	let config = db.config();
	config
		.analysis
		.attacks
		.commit
		.pr_affiliation
		.count_threshold
}

fn pr_module_contributors_active(db: &dyn CommitConfigQuery) -> bool {
	let config = db.config();
	config.analysis.attacks.commit.pr_module_contributors.active
}

fn pr_module_contributors_weight(db: &dyn CommitConfigQuery) -> u64 {
	let config = db.config();
	config.analysis.attacks.commit.pr_module_contributors.weight
}

fn pr_module_contributors_percent_threshold(db: &dyn CommitConfigQuery) -> F64 {
	let config = db.config();
	config
		.analysis
		.attacks
		.commit
		.pr_module_contributors
		.percent_threshold
}
