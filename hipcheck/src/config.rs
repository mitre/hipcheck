// SPDX-License-Identifier: Apache-2.0

//! Defines the configuration file format.

use crate::analysis::score::*;
use crate::context::Context;
use crate::engine::HcEngine;
use crate::error::Result;
use crate::hc_error;
use crate::policy::policy_file::{PolicyAnalysis, PolicyCategory, PolicyCategoryChild};
use crate::policy::PolicyFile;
use crate::util::fs as file;
use crate::BINARY_CONFIG_FILE;
use crate::F64;
use crate::LANGS_FILE;
use crate::ORGS_FILE;
use crate::TYPO_FILE;
use indextree::{Arena, NodeEdge, NodeId};
use num_traits::identities::Zero;
use pathbuf::pathbuf;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use smart_default::SmartDefault;
use std::collections::HashMap;
use std::default::Default;
use std::path::Path;
use std::path::PathBuf;
use std::rc::Rc;

impl Config {
	/// Load configuration from the given directory.
	pub fn load_from(config_path: &Path) -> Result<Config> {
		if config_path.is_file() {
			return Err(hc_error!(
				"Hipcheck config path must be a directory, not a file."
			));
		}
		let config_file = pathbuf![config_path, "Hipcheck.toml"];
		file::exists(&config_file)?;
		let config = file::read_toml(config_file).context("can't parse config file")?;

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

	/// A percentage of commits permitted to have a match between committer and
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
	/// Returns the input `Policy File` struct
	#[salsa::input]
	fn policy(&self) -> Rc<PolicyFile>;
	/// Returns the location of the policy file
	#[salsa::input]
	fn policy_path(&self) -> Option<Rc<PathBuf>>;
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
}

pub static MITRE_PUBLISHER: &str = "MITRE";
pub static LEGACY_PLUGIN: &str = "legacy";

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Analysis {
	pub publisher: String,
	pub plugin: String,
	pub query: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoliciedAnalysis(pub Analysis, pub String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnalysisTreeNode {
	Category {
		label: String,
		weight: F64,
	},
	Analysis {
		analysis: PoliciedAnalysis,
		weight: F64,
	},
}
impl AnalysisTreeNode {
	pub fn get_weight(&self) -> F64 {
		match self {
			AnalysisTreeNode::Category { weight, .. } => *weight,
			AnalysisTreeNode::Analysis { weight, .. } => *weight,
		}
	}
	pub fn normalize_weight(&mut self, divisor: F64) {
		match self {
			AnalysisTreeNode::Category { weight, .. } => {
				*weight /= divisor;
			}
			AnalysisTreeNode::Analysis { weight, .. } => {
				*weight /= divisor;
			}
		}
	}
	pub fn category(label: &str, weight: F64) -> Self {
		AnalysisTreeNode::Category {
			label: label.to_owned(),
			weight,
		}
	}
	pub fn analysis(analysis: Analysis, raw_policy: String, weight: F64) -> Self {
		AnalysisTreeNode::Analysis {
			analysis: PoliciedAnalysis(analysis, raw_policy),
			weight,
		}
	}
	pub fn augment(&self, scores: &AnalysisResults) -> Result<ScoreTreeNode> {
		match self {
			AnalysisTreeNode::Category { label, weight } => Ok(ScoreTreeNode {
				label: label.clone(),
				score: 0f64,
				weight: (*weight).into(),
			}),
			AnalysisTreeNode::Analysis { analysis, weight } => {
				let label = analysis.0.query.clone();
				let stored_res = scores.table.get(&label).ok_or(hc_error!(
					"missing expected analysis results {}",
					analysis.0.query
				))?;
				let score = stored_res.score().0;
				Ok(ScoreTreeNode {
					label,
					score: score as f64,
					weight: (*weight).into(),
				})
			}
		}
	}
	pub fn augment_plugin(&self, metrics: &HashMap<Analysis, Result<Value>>) -> ScoreTreeNode {
		match self {
			AnalysisTreeNode::Category { label, weight } => ScoreTreeNode {
				label: label.clone(),
				score: 0f64,
				weight: (*weight).into(),
			},
			AnalysisTreeNode::Analysis {
				analysis,
				weight: _,
			} => {
				let _analysis_res = metrics.get(&analysis.0);
				todo!("Extract relevant Value output from map, load into and execute policy, return score")
			}
		}
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalysisTree {
	pub tree: Arena<AnalysisTreeNode>,
	pub root: NodeId,
}
impl AnalysisTree {
	#[allow(unused)] // Will be used once RFD4 impl lands
	pub fn new(root_label: &str) -> Self {
		let mut tree = Arena::new();
		let root = tree.new_node(AnalysisTreeNode::category(
			root_label,
			F64::new(1.0).unwrap(),
		));
		AnalysisTree { tree, root }
	}
	pub fn get_analyses(&self) -> Vec<Analysis> {
		visit_leaves(
			self.root,
			&self.tree,
			|_n| false,
			|_a, n| match n {
				AnalysisTreeNode::Analysis { analysis, .. } => analysis.0.clone(),
				AnalysisTreeNode::Category { .. } => unreachable!(),
			},
		)
	}
	#[allow(unused)] // Will be used once RFD4 impl lands
	pub fn node_is_category(&self, id: NodeId) -> Result<bool> {
		let node_ref = self.tree.get(id).ok_or(hc_error!("node not in tree"))?;
		Ok(matches!(node_ref.get(), AnalysisTreeNode::Category { .. }))
	}
	#[allow(unused)] // Will be used once RFD4 impl lands
	pub fn add_category(&mut self, under: NodeId, label: &str, weight: F64) -> Result<NodeId> {
		if self.node_is_category(under)? {
			let child = self
				.tree
				.new_node(AnalysisTreeNode::category(label, weight));
			under.append(child, &mut self.tree);
			Ok(child)
		} else {
			Err(hc_error!("cannot append to analysis node"))
		}
	}
	#[allow(unused)] // Will be used once RFD4 impl lands
	pub fn add_analysis(
		&mut self,
		under: NodeId,
		analysis: Analysis,
		raw_policy: String,
		weight: F64,
	) -> Result<NodeId> {
		if self.node_is_category(under)? {
			let child = self
				.tree
				.new_node(AnalysisTreeNode::analysis(analysis, raw_policy, weight));
			under.append(child, &mut self.tree);
			Ok(child)
		} else {
			Err(hc_error!("cannot append to analysis node"))
		}
	}
	// @Temporary - WeightTree will be replaced by AnalysisTree, and WeightTree and this function
	// will cease to exit
	#[allow(dead_code)]
	pub fn from_weight_tree(weight_tree: &WeightTree) -> Result<Self> {
		use indextree::NodeEdge::*;
		let mut tree = Arena::<AnalysisTreeNode>::new();
		let weight_root = weight_tree.root;
		let analysis_root = tree.new_node(
			weight_tree
				.tree
				.get(weight_root)
				.ok_or(hc_error!("WeightTree root not in tree, invalid state"))?
				.get()
				.as_category_node(),
		);

		let mut scope: Vec<NodeId> = vec![analysis_root];
		for edge in weight_root.traverse(&weight_tree.tree) {
			match edge {
				Start(n) => {
					let curr_weight_node = weight_tree
						.tree
						.get(n)
						.ok_or(hc_error!("WeightTree root not in tree, invalid state"))?
						.get();
					// If is a category node
					let curr_node = if n.children(&weight_tree.tree).next().is_some() {
						tree.new_node(curr_weight_node.as_category_node())
					} else {
						tree.new_node(curr_weight_node.with_hardcoded_expr())
					};
					scope
						.last()
						.ok_or(hc_error!("Scope stack is empty, invalid state"))?
						.append(curr_node, &mut tree);
					scope.push(curr_node);
				}
				End(_) => {
					scope.pop();
				}
			};
		}
		Ok(AnalysisTree {
			tree,
			root: analysis_root,
		})
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeightTreeNode {
	pub label: String,
	pub weight: F64,
}
impl WeightTreeNode {
	pub fn new(label: &str, weight: F64) -> Self {
		WeightTreeNode {
			label: label.to_owned(),
			weight,
		}
	}
	#[allow(unused)]
	pub fn augment(&self, scores: &AnalysisResults) -> ScoreTreeNode {
		let score = match scores.table.get(&self.label) {
			Some(res) => res.score().0,
			_ => 0,
		};
		ScoreTreeNode {
			label: self.label.clone(),
			score: score as f64,
			weight: self.weight.into(),
		}
	}
	#[allow(dead_code)]
	pub fn as_category_node(&self) -> AnalysisTreeNode {
		AnalysisTreeNode::Category {
			label: self.label.clone(),
			weight: self.weight,
		}
	}
	// @Temporary - until policy file impl'd and integrated, we hard-code
	// the policy for our analyses
	#[allow(dead_code)]
	pub fn with_hardcoded_expr(&self) -> AnalysisTreeNode {
		let expr = "true".to_owned();
		let analysis = Analysis {
			publisher: MITRE_PUBLISHER.to_owned(),
			plugin: LEGACY_PLUGIN.to_owned(),
			query: self.label.clone(),
		};
		AnalysisTreeNode::Analysis {
			analysis: PoliciedAnalysis(analysis, expr),
			weight: self.weight,
		}
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeightTree {
	pub tree: Arena<WeightTreeNode>,
	pub root: NodeId,
}
impl WeightTree {
	pub fn new(root_label: &str) -> Self {
		let mut tree = Arena::new();
		let root = tree.new_node(WeightTreeNode::new(root_label, F64::new(1.0).unwrap()));
		WeightTree { tree, root }
	}
	pub fn add_child_u64(&mut self, under: NodeId, label: &str, weight: u64) -> NodeId {
		let weight = F64::new(weight as f64).unwrap();
		self.add_child(under, label, weight)
	}
	pub fn add_child(&mut self, under: NodeId, label: &str, weight: F64) -> NodeId {
		let child = self.tree.new_node(WeightTreeNode::new(label, weight));
		under.append(child, &mut self.tree);
		child
	}
}

// Generic function for visiting and performing operations on an indexmap::Arena.
// A function `acc_op` is applied to each node, and the results this function build up a
// "scope" which is a vector of `acc_op` output from the root node to the current node.
// When a leaf node is detected, `chil_op` is called, and the function receives both
// the current node and a slice-view of the scope vector. The output of calling `chil_op`
// on each leaf node is aggregated and returned.
pub fn visit_leaves<T, A: Clone, B, F1, F2>(
	node: NodeId,
	tree: &Arena<T>,
	acc_op: F1,
	chil_op: F2,
) -> Vec<B>
where
	F1: Fn(&T) -> A,
	F2: Fn(&[A], &T) -> B,
{
	let mut scope: Vec<A> = vec![];
	let mut last_start: NodeId = node;
	let mut out_vals: Vec<B> = vec![];
	for edge in node.traverse(tree) {
		match edge {
			// Entering a new scope, update the tracker vec
			NodeEdge::Start(n) => {
				last_start = n;
				scope.push(acc_op(tree.get(n).unwrap().get()));
			}
			NodeEdge::End(n) => {
				// If we just saw Start on the same NodeId, this is a leaf
				if n == last_start {
					let node = tree.get(n).unwrap().get();
					out_vals.push(chil_op(scope.as_slice(), node));
				}
				scope.pop();
			}
		}
	}
	out_vals
}

#[salsa::query_group(WeightTreeQueryStorage)]
pub trait WeightTreeProvider:
	FuzzConfigQuery + PracticesConfigQuery + AttacksConfigQuery + CommitConfigQuery + HcEngine
{
	/// Returns the tree of raw analysis weights from the config
	fn weight_tree(&self) -> Result<Rc<WeightTree>>;
	/// Returns the tree of normalized weights for analyses from the config
	fn normalized_weight_tree(&self) -> Result<Rc<WeightTree>>;

	/// Returns the weight tree including policy expressions
	fn analysis_tree(&self) -> Result<Rc<AnalysisTree>>;
	/// Returns the tree of normalized weights for analyses from the config
	fn normalized_analysis_tree(&self) -> Result<Rc<AnalysisTree>>;
}

fn add_analysis(
	core: &dyn WeightTreeProvider,
	tree: &mut AnalysisTree,
	under: NodeId,
	analysis: PolicyAnalysis,
) -> Result<NodeId> {
	let publisher = analysis.name.publisher;
	let plugin = analysis.name.name;
	let weight = match analysis.weight {
		Some(u) => F64::new(u as f64)?,
		None => F64::new(1.0)?,
	};
	let raw_policy = match analysis.policy_expression {
        Some(x) => x,
        None => core.default_policy_expr(publisher.clone(), plugin.clone())?.ok_or(hc_error!("plugin {}::{} does not have a default policy, please define a policy in your policy file"))?
    };
	let analysis = Analysis {
		publisher,
		plugin,
		query: "default".to_owned(),
	};
	tree.add_analysis(under, analysis, raw_policy, weight)
}

fn add_category(
	core: &dyn WeightTreeProvider,
	tree: &mut AnalysisTree,
	under: NodeId,
	category: &PolicyCategory,
) -> Result<NodeId> {
	let weight = F64::new(match category.weight {
		Some(w) => w as f64,
		None => 1.0,
	})
	.unwrap();
	let id = tree.add_category(under, category.name.as_str(), weight)?;
	for c in category.children.iter() {
		match c {
			PolicyCategoryChild::Analysis(analysis) => {
				add_analysis(core, tree, id, analysis.clone())?;
			}
			PolicyCategoryChild::Category(category) => {
				add_category(core, tree, id, category)?;
			}
		}
	}
	Ok(id)
}

pub fn analysis_tree(db: &dyn WeightTreeProvider) -> Result<Rc<AnalysisTree>> {
	let policy = db.policy();
	let mut tree = AnalysisTree::new("risk");
	let root = tree.root;

	for c in policy.analyze.categories.iter() {
		add_category(db, &mut tree, root, c)?;
	}

	Ok(Rc::new(tree))
}

pub fn weight_tree(db: &dyn WeightTreeProvider) -> Result<Rc<WeightTree>> {
	let mut tree = WeightTree::new(RISK_PHASE);
	if db.practices_active() {
		let practices_node = tree.add_child_u64(tree.root, PRACTICES_PHASE, db.practices_weight());
		tree.add_child_u64(practices_node, ACTIVITY_PHASE, db.activity_weight());
		tree.add_child_u64(practices_node, REVIEW_PHASE, db.review_weight());
		tree.add_child_u64(practices_node, BINARY_PHASE, db.binary_weight());
		tree.add_child_u64(practices_node, IDENTITY_PHASE, db.identity_weight());
		tree.add_child_u64(practices_node, FUZZ_PHASE, db.fuzz_weight());
	}
	if db.attacks_active() {
		let attacks_node = tree.add_child_u64(tree.root, ATTACKS_PHASE, db.attacks_weight());
		tree.add_child_u64(attacks_node, TYPO_PHASE, db.typo_weight());
		if db.commit_active() {
			let commit_node = tree.add_child_u64(attacks_node, COMMITS_PHASE, db.commit_weight());
			tree.add_child_u64(commit_node, AFFILIATION_PHASE, db.affiliation_weight());
			tree.add_child_u64(commit_node, CHURN_PHASE, db.churn_weight());
			tree.add_child_u64(commit_node, ENTROPY_PHASE, db.entropy_weight());
		}
	}
	Ok(Rc::new(tree))
}

fn normalize_at_internal(node: NodeId, tree: &mut Arena<AnalysisTreeNode>) -> F64 {
	let children: Vec<NodeId> = node.children(tree).collect();
	let weight_sum: F64 = children
		.iter()
		.map(|n| normalize_at_internal(*n, tree))
		.sum();
	if !weight_sum.is_zero() {
		for c in children {
			let child = tree.get_mut(c).unwrap().get_mut();
			child.normalize_weight(weight_sum);
		}
	}
	tree.get(node).unwrap().get().get_weight()
}

pub fn normalized_analysis_tree(db: &dyn WeightTreeProvider) -> Result<Rc<AnalysisTree>> {
	let tree = db.analysis_tree();
	let mut norm_tree: AnalysisTree = (*tree?).clone();
	normalize_at_internal(norm_tree.root, &mut norm_tree.tree);
	Ok(Rc::new(norm_tree))
}

fn normalize_wt_internal(node: NodeId, tree: &mut Arena<WeightTreeNode>) -> F64 {
	let children: Vec<NodeId> = node.children(tree).collect();
	let weight_sum: F64 = children
		.iter()
		.map(|n| normalize_wt_internal(*n, tree))
		.sum();
	if !weight_sum.is_zero() {
		for c in children {
			let child = tree.get_mut(c).unwrap().get_mut();
			child.weight /= weight_sum;
		}
	}
	tree.get(node).unwrap().get().weight
}

pub fn normalized_weight_tree(db: &dyn WeightTreeProvider) -> Result<Rc<WeightTree>> {
	let tree = db.weight_tree();
	let mut norm_tree: WeightTree = (*tree?).clone();
	normalize_wt_internal(norm_tree.root, &mut norm_tree.tree);
	Ok(Rc::new(norm_tree))
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
