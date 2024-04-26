// SPDX-License-Identifier: Apache-2.0

//! Defines the configuration file format.

mod query;

pub use query::*;

use hc_common::{
	context::{Context, Result},
	serde::{self, Deserialize, Serialize},
	F64,
};
use hc_filesystem as file;
use smart_default::SmartDefault;
use std::default::Default;
use std::path::Path;

impl Config {
	/// Load configuration from the given directory.
	pub fn load_from(config_path: &Path) -> Result<Config> {
		file::exists(&config_path)?;
		let config = file::read_toml(&config_path).context("can't parse config file")?;

		Ok(config)
	}
}

/// Represents the configuration of Hipcheck's analyses.
#[derive(Debug, Deserialize, Serialize, Default, PartialEq, Eq)]
#[serde(crate = "self::serde")]
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
#[serde(default, crate = "self::serde")]
pub struct RiskConfig {
	/// The risk tolerance threshold, a value between 0 and 1.
	#[default(_code = "F64::new(0.5).unwrap()")]
	#[serde(deserialize_with = "de::percent")]
	pub threshold: F64,
}

/// Defines configuration for all of Hipcheck's analyses.
#[derive(Debug, Deserialize, Serialize, Default, PartialEq, Eq)]
#[serde(default, crate = "self::serde")]
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
#[serde(default, crate = "self::serde")]
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
#[serde(default, crate = "self::serde")]
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
#[serde(default, crate = "self::serde")]
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
#[serde(default, crate = "self::serde")]
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
#[serde(default, crate = "self::serde")]
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
#[serde(default, crate = "self::serde")]
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
#[serde(default, crate = "self::serde")]
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
#[serde(default, crate = "self::serde")]
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
#[serde(default, crate = "self::serde")]
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
#[serde(default, crate = "self::serde")]
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
#[serde(default, crate = "self::serde")]
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
#[serde(default, crate = "self::serde")]
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
#[serde(default, crate = "self::serde")]
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
#[serde(default, crate = "self::serde")]
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
#[serde(default, crate = "self::serde")]
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
#[serde(crate = "self::serde")]
pub struct LanguagesConfig {
	/// The file to pull language information from.
	#[default = "Langs.toml"]
	pub langs_file: String,
}

/// Defines configuration for fuzz analysis.
#[derive(Debug, Deserialize, Serialize, SmartDefault, PartialEq, Eq)]
#[serde(default, crate = "self::serde")]
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
	use super::serde::de::{self, Deserializer, Visitor};
	use super::F64;
	use std::fmt::{self, Formatter};

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
