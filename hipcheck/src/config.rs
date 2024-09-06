// SPDX-License-Identifier: Apache-2.0

//! Defines the configuration file format.

use crate::analysis::score::*;
use crate::context::Context;
use crate::engine::HcEngine;
use crate::error::Result;
use crate::hc_error;
use crate::policy::policy_file::{PolicyAnalysis, PolicyCategory, PolicyCategoryChild};
use crate::policy::PolicyFile;
use crate::policy_exprs::Executor;
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
	/// Returns the directory containing the config file (deprecated)
	#[salsa::input]
	fn config_dir(&self) -> Option<Rc<PathBuf>>;
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
	fn langs_file(&self) -> Result<Rc<PathBuf>>;
}

/// Queries for accessing the practices analysis config
#[salsa::query_group(PracticesConfigQueryStorage)]
pub trait PracticesConfigQuery: ConfigSource {
	/// Returns the binary formats file path relative to the
	/// config file
	fn binary_formats_file_rel(&self) -> Rc<String>;
	/// Returns the binary formats file absolute path
	fn binary_formats_file(&self) -> Result<Rc<PathBuf>>;
}

/// Queries for accessing the attacks analysis config
#[salsa::query_group(AttacksConfigQueryStorage)]
pub trait AttacksConfigQuery: CommitConfigQuery {
	/// Returns the typo file path relative to the config file
	fn typo_file_rel(&self) -> Rc<String>;
	/// Returns the typo file absolute path
	fn typo_file(&self) -> Result<Rc<PathBuf>>;
}

/// Queries for accessing the commit analysis config
#[salsa::query_group(CommitConfigQueryStorage)]
pub trait CommitConfigQuery: ConfigSource {
	/// Returns the orgs file path relative to the config file
	fn orgs_file_rel(&self) -> Rc<String>;
	/// Returns the orgs file absolute path
	fn orgs_file(&self) -> Result<Rc<PathBuf>>;
	/// Returns the contributor trust analysis count threshold
	fn contributor_trust_value_threshold(&self) -> u64;
	/// Returns the contributor trust analysis month threshold
	fn contributor_trust_month_count_threshold(&self) -> u64;
}

pub static MITRE_PUBLISHER: &str = "mitre";
pub static DEFAULT_QUERY: &str = "";

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
	pub fn get_print_label(&self) -> String {
		match self {
			AnalysisTreeNode::Category { label, .. } => label.clone(),
			AnalysisTreeNode::Analysis { analysis, .. } => {
				format!("{}::{}", analysis.0.publisher, analysis.0.plugin)
			}
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
			AnalysisTreeNode::Analysis { analysis, weight } => {
				let Some(analysis_res) = metrics.get(&analysis.0) else {
					panic!(
						"missing expected analysis results for {}",
						self.get_print_label()
					);
				};
				let label = self.get_print_label();
				let weight = (*weight).into();
				match analysis_res {
					Ok(output) => {
						println!("expr: {}", analysis.1);
						println!("context: {:?}", &output);
						let score = match Executor::std().run(analysis.1.as_str(), &output) {
							Ok(true) => 0.0,
							Ok(false) => 1.0,
							Err(e) => {
								panic!("policy evaluation failed: {e}");
							}
						};
						ScoreTreeNode {
							label,
							score,
							weight,
						}
					}
					Err(e) => {
						println!("Analysis error: {e}");
						ScoreTreeNode {
							label,
							score: 1f64,
							weight,
						}
					}
				}
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
pub trait WeightTreeProvider: ConfigSource + HcEngine {
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
		query: DEFAULT_QUERY.to_owned(),
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

/// Derived query implementations

/// In general, these simply return the value of a particular field in
/// one of the `Config` child structs.  When the type of the desired
/// field is `String`, it is returned wrapped in an `Rc`.  This is
/// done to keep Salsa's cloning cheap.

#[allow(unused_variables)]
fn risk_threshold(db: &dyn RiskConfigQuery) -> F64 {
	// @Todo - change signature to return string representation of policy expr from policy file
	F64::new(0.5).unwrap()
}

fn langs_file_rel(_db: &dyn LanguagesConfigQuery) -> Rc<String> {
	Rc::new(LANGS_FILE.to_string())
}

fn langs_file(db: &dyn LanguagesConfigQuery) -> Result<Rc<PathBuf>> {
	if let Some(config_dir) = db.config_dir() {
		Ok(Rc::new(pathbuf![config_dir.as_ref(), db.langs_file_rel().as_ref()]))
	} else {
		let policy_file = db.policy();
		for category in  &policy_file.as_ref().analyze.categories {
			if category.name.eq("languages") {
				for child in &category.children {
					match child {
						PolicyCategoryChild::Analysis(analysis) => {
							if analysis.name.name == "linguist" {
								if let Some(config) = &analysis.config {
									if let Some(filepath) = config.clone().get("langs-file") {
										return Ok(Rc::new(Path::new(&filepath).to_path_buf()))
									}
								}
							}
						},
						_ => return Err(hc_error!("Cannot find path to languages config file in policy file. This file is necessary for running the linguist analysis."))
					}
				}
			}
		}
		Err(hc_error!("Cannot find path to languages config file in policy file. This file is necessary for running the linguist analysis."))
	}
}

fn binary_formats_file_rel(_db: &dyn PracticesConfigQuery) -> Rc<String> {
	Rc::new(BINARY_CONFIG_FILE.to_string())
}

fn binary_formats_file(db: &dyn PracticesConfigQuery) -> Result<Rc<PathBuf>> {
	if let Some(config_dir) = db.config_dir() {
		Ok(Rc::new(pathbuf![
			config_dir.as_ref(),
			db.binary_formats_file_rel().as_ref()
		]))
	} else {
		let policy_file = db.policy();
		for category in  &policy_file.as_ref().analyze.categories {
			if category.name.eq("practices") {
				for child in &category.children {
					match child {
						PolicyCategoryChild::Analysis(analysis) => {
							if analysis.name.name == "binary" {
								if let Some(config) = &analysis.config {
									if let Some(filepath) = config.clone().get("binary-file") {
										return Ok(Rc::new(Path::new(&filepath).to_path_buf()))
									}
								}
							}
						},
						_ => return Err(hc_error!("Cannot find path to bonary formats config file in policy file. This file is necessary for running the binary analysis."))
					}
				}
			}
		}
		Err(hc_error!("Cannot find path to binary format config file in policy file. This file is necessary for running the binary analysis."))
	}
}

fn typo_file_rel(_db: &dyn AttacksConfigQuery) -> Rc<String> {
	Rc::new(TYPO_FILE.to_string())
}

fn typo_file(db: &dyn AttacksConfigQuery) -> Result<Rc<PathBuf>> {
	if let Some(config_dir) = db.config_dir() {
		Ok(Rc::new(pathbuf![config_dir.as_ref(), db.typo_file_rel().as_ref()]))
	} else {
		let policy_file = db.policy();
		for category in  &policy_file.as_ref().analyze.categories {
			if category.name.eq("attacks") {
				for child in &category.children {
					match child {
						PolicyCategoryChild::Analysis(analysis) => {
							if analysis.name.name == "typo" {
								if let Some(config) = &analysis.config {
									if let Some(filepath) = config.clone().get("typo-file") {
										return Ok(Rc::new(Path::new(&filepath).to_path_buf()))
									}
								}
							}
						},
						_ => return Err(hc_error!("Cannot find path to typos config file in policy file. This file is necessary for running the typo analysis."))
					}
				}
			}
		}
		Err(hc_error!("Cannot find path to typo config file in policy file. This file is necessary for running the typo analysis."))
	}
}

fn orgs_file_rel(_db: &dyn CommitConfigQuery) -> Rc<String> {
	Rc::new(ORGS_FILE.to_string())
}

fn orgs_file(db: &dyn CommitConfigQuery) -> Result<Rc<PathBuf>> {
	if let Some(config_dir) = db.config_dir() {
		Ok(Rc::new(pathbuf![config_dir.as_ref(), db.orgs_file_rel().as_ref()]))
	} else {
		let policy_file = db.policy();
		for category in  &policy_file.as_ref().analyze.categories {
			if category.name.eq("attacks") {
				for child in &category.children {
					match child {
						PolicyCategoryChild::Category(child_category) => {
							if child_category.name.eq("commit") {
								for child in &child_category.children {
									match child {
										PolicyCategoryChild::Analysis(analysis) => {
											if analysis.name.name == "affiliation" {
												if let Some(config) = &analysis.config {
													if let Some(filepath) = config.clone().get("orgs-file") {
														return Ok(Rc::new(Path::new(&filepath).to_path_buf()))
													}
												}
											}
										},
										_ => return Err(hc_error!("Cannot find path to orgs config file in policy file. This file is necessary for running the affiliation analysis."))
									}
								}
							}
						},
						_ => return Err(hc_error!("Cannot find path to orgs config file in policy file. This file is necessary for running the affiliation analysis."))
					}
				}
			}
		}
		Err(hc_error!("Cannot find path to orgs config file in policy file. This file is necessary for running the affiliation analysis."))
	}
}

fn contributor_trust_value_threshold(db: &dyn CommitConfigQuery) -> u64 {
	todo!("back it out from policy file config")
}

fn contributor_trust_month_count_threshold(db: &dyn CommitConfigQuery) -> u64 {
	todo!("back it out from policy file config")
}
