// SPDX-License-Identifier: Apache-2.0

//! Defines the configuration file format.

use crate::{
	engine::HcEngine,
	error::{Context, Result},
	hc_error,
	policy::{
		policy_file::{PolicyAnalysis, PolicyCategory, PolicyCategoryChild},
		PolicyFile,
	},
	policy_exprs::Expr,
	score::*,
	session::Session,
	util::fs as file,
	F64,
};
use indextree::{Arena, NodeEdge, NodeId};
use num_traits::identities::Zero;
use pathbuf::pathbuf;
use serde::{Deserialize, Serialize};
use smart_default::SmartDefault;
use std::{collections::HashMap, default::Default, path::Path, rc::Rc};

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
	#[default = "Orgs.kdl"]
	pub orgs_file: String,
}

/// Defines configuration for binary file analysis.
#[derive(Debug, Deserialize, Serialize, SmartDefault, PartialEq, Eq)]
#[serde(default)]
pub struct BinaryConfig {
	/// Binary file extension configuration file.
	#[default = "Binary.kdl"]
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
	#[default = "Typos.kdl"]
	pub typo_file: String,
}

/// Defines the configuration of language-specific info.
#[derive(Debug, Deserialize, Serialize, SmartDefault, PartialEq, Eq)]
#[serde(default)]
pub struct LanguagesConfig {
	/// The file to pull language information from.
	#[default = "Langs.kdl"]
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
	use serde::{
		de,
		de::{Deserializer, Visitor},
	};
	use std::{fmt, fmt::Formatter};

	/// Deserialize a float, ensuring it's between 0.0 and 1.0 inclusive.
	pub(super) fn percent<'de, D>(deserializer: D) -> Result<F64, D::Error>
	where
		D: Deserializer<'de>,
	{
		struct PercentVisitor;

		impl Visitor<'_> for PercentVisitor {
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

pub static DEFAULT_QUERY: &str = "";

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Analysis {
	pub publisher: String,
	pub plugin: String,
	pub query: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoliciedAnalysis(pub Analysis, pub Option<Expr>);

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
	pub fn analysis(analysis: Analysis, opt_policy: Option<Expr>, weight: F64) -> Self {
		AnalysisTreeNode::Analysis {
			analysis: PoliciedAnalysis(analysis, opt_policy),
			weight,
		}
	}
	pub fn augment_plugin(
		&self,
		metrics: &HashMap<Analysis, PluginAnalysisResult>,
	) -> ScoreTreeNode {
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
				let score = match analysis_res.passed {
					true => 0.0,
					false => 1.0,
				};
				ScoreTreeNode {
					label,
					score,
					weight,
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
	pub fn new(root_label: &str) -> Self {
		let mut tree = Arena::new();
		let root = tree.new_node(AnalysisTreeNode::category(
			root_label,
			F64::new(1.0).unwrap(),
		));
		AnalysisTree { tree, root }
	}

	pub fn get_analyses(&self) -> Vec<PoliciedAnalysis> {
		visit_leaves(
			self.root,
			&self.tree,
			|_n| false,
			|_a, n| match n {
				AnalysisTreeNode::Analysis { analysis, .. } => analysis.clone(),
				AnalysisTreeNode::Category { .. } => unreachable!(),
			},
		)
	}

	pub fn node_is_category(&self, id: NodeId) -> Result<bool> {
		let node_ref = self.tree.get(id).ok_or(hc_error!("node not in tree"))?;
		Ok(matches!(node_ref.get(), AnalysisTreeNode::Category { .. }))
	}

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

	pub fn add_analysis(
		&mut self,
		under: NodeId,
		analysis: Analysis,
		opt_policy: Option<Expr>,
		weight: F64,
	) -> Result<NodeId> {
		if self.node_is_category(under)? {
			let child = self
				.tree
				.new_node(AnalysisTreeNode::analysis(analysis, opt_policy, weight));
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

// Generic function for visiting and performing operations on an indexmap::Arena.
// A function `acc_op` is applied to each node, and the results this function build up a
// "scope" which is a vector of `acc_op` output from the root node to the current node.
// When a leaf node is detected, `chil_op` is called, and the function receives both
// the current node and a slice-view of the scope vector. The output of calling `chil_op`
// on each leaf node is aggregated and returned.
pub fn mutate_leaves<T, F>(node: NodeId, tree: &mut Arena<T>, op: F) -> Result<()>
where
	F: Fn(&mut T) -> Result<()>,
{
	let mut last_start: NodeId = node;
	let edges: Vec<_> = node.traverse(tree).collect();
	for edge in edges {
		match edge {
			// Entering a new scope, update the tracker vec
			NodeEdge::Start(n) => {
				last_start = n;
			}
			NodeEdge::End(n) => {
				// If we just saw Start on the same NodeId, this is a leaf
				if n == last_start {
					let node = tree.get_mut(n).unwrap().get_mut();
					op(node)?;
				}
			}
		}
	}
	Ok(())
}

fn add_analysis(
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
	let opt_policy = analysis
		.policy_expression
		.map(|s| s.parse::<Expr>())
		.transpose()?;
	let analysis = Analysis {
		publisher: publisher.0,
		plugin: plugin.0,
		query: DEFAULT_QUERY.to_owned(),
	};
	tree.add_analysis(under, analysis, opt_policy, weight)
}

fn add_category(
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
				add_analysis(tree, id, analysis.clone())?;
			}
			PolicyCategoryChild::Category(category) => {
				add_category(tree, id, category)?;
			}
		}
	}
	Ok(id)
}

// Lowest-level function to turn a PolicyFile into an AnalysisTree
pub fn unresolved_analysis_tree_from_policy(policy: &PolicyFile) -> Result<AnalysisTree> {
	let mut tree = AnalysisTree::new(&policy.analyze.root.name);
	let root = tree.root;
	add_category(&mut tree, root, &policy.analyze.root)?;

	Ok(tree)
}

pub fn unresolved_analysis_tree(session: &Session) -> Result<Rc<AnalysisTree>> {
	let policy = session.policy_file();
	unresolved_analysis_tree_from_policy(policy).map(Rc::new)
}

pub fn analysis_tree(session: &Session) -> Result<Rc<AnalysisTree>> {
	let unresolved_tree = normalized_unresolved_analysis_tree(session)?;
	let mut res_tree: AnalysisTree = (*unresolved_tree).clone();

	// If the policy is empty, try to look up from plugin engine
	let update_policy = |node: &mut AnalysisTreeNode| -> Result<()> {
		if let AnalysisTreeNode::Analysis { analysis, .. } = node {
			let a: &Analysis = &analysis.0;
			if analysis.1.is_none() {
				analysis.1 = Some(session.default_policy_expr(a.publisher.clone(), a.plugin.clone())?.ok_or(hc_error!("plugin {}::{} does not have a default policy, please define a policy in your policy file", a.publisher.clone(), a.plugin.clone()))?);
			}
		}
		Ok(())
	};

	// Walk the tree, applying the above closure to each leaf (i.e. Analysis) node
	mutate_leaves(res_tree.root, &mut res_tree.tree, update_policy)?;

	Ok(Rc::new(res_tree))
}

// Recursive implementation of tree weight normalization
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

pub fn normalized_unresolved_analysis_tree_from_policy(
	policy: &PolicyFile,
) -> Result<Rc<AnalysisTree>> {
	let mut tree = unresolved_analysis_tree_from_policy(policy)?;
	normalize_at_internal(tree.root, &mut tree.tree);
	Ok(Rc::new(tree))
}

pub fn normalized_unresolved_analysis_tree(session: &Session) -> Result<Rc<AnalysisTree>> {
	let tree = unresolved_analysis_tree(session)?;
	let mut norm_tree: AnalysisTree = (*tree).clone();
	normalize_at_internal(norm_tree.root, &mut norm_tree.tree);
	Ok(Rc::new(norm_tree))
}
