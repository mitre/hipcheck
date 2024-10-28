// SPDX-License-Identifier: Apache-2.0

use crate::{
	analysis::AnalysisProvider,
	config::{
		visit_leaves, Analysis, AnalysisTree, WeightTreeProvider, DEFAULT_QUERY, MITRE_PUBLISHER,
	},
	engine::HcEngine,
	error::Result,
	hc_error,
	plugin::{QueryResult, MITRE_LEGACY_PLUGINS},
	policy_exprs::Executor,
	shell::spinner_phase::SpinnerPhase,
};
use indextree::{Arena, NodeId};
#[cfg(test)]
use num_traits::identities::Zero;
use serde_json::Value;
use std::{collections::HashMap, default::Default};

#[cfg(test)]
pub const PRACTICES_PHASE: &str = "practices";
#[cfg(test)]
pub const ATTACKS_PHASE: &str = "attacks";
#[cfg(test)]
pub const COMMITS_PHASE: &str = "commits";

pub const REVIEW_PHASE: &str = "review";
pub const IDENTITY_PHASE: &str = "identity";
pub const BINARY_PHASE: &str = "binary";
pub const ACTIVITY_PHASE: &str = "activity";
pub const FUZZ_PHASE: &str = "fuzz";
pub const TYPO_PHASE: &str = "typo";
pub const AFFILIATION_PHASE: &str = "affiliation";
pub const CHURN_PHASE: &str = "churn";
pub const ENTROPY_PHASE: &str = "entropy";

#[derive(Debug, Default)]
pub struct ScoringResults {
	pub results: PluginAnalysisResults,
	pub score: Score,
}

#[derive(Debug, Clone)]
pub struct PluginAnalysisResult {
	pub response: Result<QueryResult>,
	pub policy: String,
	pub passed: bool,
}

#[derive(Debug, Default)]
pub struct PluginAnalysisResults {
	pub table: HashMap<Analysis, PluginAnalysisResult>,
}

impl PluginAnalysisResults {
	pub fn get_legacy(&self, analysis: &str) -> Option<&PluginAnalysisResult> {
		if MITRE_LEGACY_PLUGINS.contains(&analysis) {
			let key = Analysis::legacy(analysis);
			self.table.get(&key)
		} else {
			None
		}
	}
	/// Get all results from non-legacy analyses.
	pub fn plugin_results(&self) -> impl Iterator<Item = (&Analysis, &PluginAnalysisResult)> {
		self.table.iter().filter_map(|(analysis, result)| {
			if MITRE_LEGACY_PLUGINS.contains(&analysis.plugin.as_str())
				&& analysis.publisher == MITRE_PUBLISHER
			{
				None
			} else {
				Some((analysis, result))
			}
		})
	}
}

#[derive(Debug, Default)]
pub struct Score {
	pub total: f64,
}

#[salsa::query_group(ScoringProviderStorage)]
pub trait ScoringProvider: HcEngine + AnalysisProvider + WeightTreeProvider {
	fn wrapped_query(
		&self,
		publisher: String,
		plugin: String,
		query: String,
		key: Value,
	) -> Result<QueryResult>;
}

#[cfg(test)]
fn normalize_st_internal(node: NodeId, tree: &mut Arena<ScoreTreeNode>) -> f64 {
	let children: Vec<NodeId> = node.children(tree).collect();
	let weight_sum: f64 = children
		.iter()
		.map(|n| normalize_st_internal(*n, tree))
		.sum();
	if !weight_sum.is_zero() {
		for c in children {
			let child = tree.get_mut(c).unwrap().get_mut();
			child.weight /= weight_sum;
		}
	}
	tree.get(node).unwrap().get().weight
}

#[derive(Debug, Clone)]
pub struct ScoreTree {
	pub tree: Arena<ScoreTreeNode>,
	pub root: NodeId,
}

impl ScoreTree {
	#[cfg(test)]
	pub fn new(root_label: &str) -> Self {
		let mut tree = Arena::<ScoreTreeNode>::new();
		let root = tree.new_node(ScoreTreeNode {
			label: root_label.to_owned(),
			score: -1.0,
			weight: 1.0,
		});
		ScoreTree { tree, root }
	}

	#[cfg(test)]
	pub fn add_child(&mut self, under: NodeId, label: &str, score: f64, weight: f64) -> NodeId {
		let child = self.tree.new_node(ScoreTreeNode {
			label: label.to_owned(),
			score,
			weight,
		});
		under.append(child, &mut self.tree);
		child
	}

	#[cfg(test)]
	pub fn normalize(mut self) -> Self {
		let _ = normalize_st_internal(self.root, &mut self.tree);
		self
	}

	pub fn synthesize_plugin(
		analysis_tree: &AnalysisTree,
		scores: &PluginAnalysisResults,
	) -> Result<Self> {
		use indextree::NodeEdge::*;
		let mut tree = Arena::<ScoreTreeNode>::new();
		let analysis_root = analysis_tree.root;
		let score_root = tree.new_node(
			analysis_tree
				.tree
				.get(analysis_root)
				.ok_or(hc_error!("AnalysisTree root not in tree, invalid state"))?
				.get()
				.augment_plugin(&scores.table),
		);

		let mut scope: Vec<NodeId> = vec![score_root];
		for edge in analysis_root.traverse(&analysis_tree.tree) {
			match edge {
				Start(n) => {
					let curr_node = tree.new_node(
						analysis_tree
							.tree
							.get(n)
							.ok_or(hc_error!("AnalaysisTree node not in tree, invalid state"))?
							.get()
							.augment_plugin(&scores.table),
					);
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

		Ok(ScoreTree {
			tree,
			root: score_root,
		})
	}

	// As our scope, we track the weight of each node. Once we get to a leaf node, we multiply all
	// the weights (already normalized) by the score (0/1) then sum each value
	pub fn score(&self) -> f64 {
		let raw_score = visit_leaves(
			self.root,
			&self.tree,
			|n| n.weight,
			|a, n| a.iter().product::<f64>() * n.score,
		)
		.into_iter()
		.sum();
		decimal_truncate(raw_score)
	}
}

//stores the score tree using petgraph
//the tree does not need to know what sections it is scoring
#[derive(Debug, Clone)]
pub struct ScoreTreeNode {
	#[allow(unused)]
	pub label: String,
	pub score: f64,
	pub weight: f64,
}

fn wrapped_query(
	db: &dyn ScoringProvider,
	publisher: String,
	plugin: String,
	query: String,
	key: Value,
) -> Result<QueryResult> {
	if publisher == *MITRE_PUBLISHER && MITRE_LEGACY_PLUGINS.contains(&plugin.as_str()) {
		if query != *DEFAULT_QUERY {
			return Err(hc_error!("legacy analyses only have a default query"));
		}
		match plugin.as_str() {
			ACTIVITY_PHASE => db.activity_analysis(),
			AFFILIATION_PHASE => db.affiliation_analysis(),
			BINARY_PHASE => db.binary_analysis(),
			CHURN_PHASE => db.churn_analysis(),
			ENTROPY_PHASE => db.entropy_analysis(),
			IDENTITY_PHASE => db.identity_analysis(),
			FUZZ_PHASE => db.fuzz_analysis(),
			REVIEW_PHASE => db.review_analysis(),
			TYPO_PHASE => db.typo_analysis(),
			error => Err(hc_error!("Unrecognized legacy analysis '{}'", error)),
		}
	} else {
		db.query(publisher, plugin, query, key)
	}
}

pub fn score_results(_phase: &SpinnerPhase, db: &dyn ScoringProvider) -> Result<ScoringResults> {
	// Scoring should be performed by the construction of a "score tree" where scores are the
	// nodes and weights are the edges. The leaves are the analyses themselves, which either
	// pass (a score of 0) or fail (a score of 1). These are then combined with the other
	// children of their parent according to their weights, repeating until the final score is
	// reached.
	//
	// Values set with -1.0 are reseved for parent nodes whose score comes always
	// from children nodes with a score set by hc_analysis algorithms

	let analysis_tree = db.normalized_analysis_tree()?;
	let mut plugin_results = PluginAnalysisResults::default();

	// RFD4 analysis style - get all "leaf" analyses and call through plugin architecture
	let plugin_score_tree = {
		let target_json = serde_json::to_value(db.target().as_ref())?;

		for analysis in analysis_tree.get_analyses() {
			// Perform query, passing target in JSON
			let response = db.wrapped_query(
				analysis.0.publisher.clone(),
				analysis.0.plugin.clone(),
				analysis.0.query.clone(),
				target_json.clone(),
			);

			// Determine if analysis passed by evaluating policy expr
			let passed = {
				if let Ok(output) = &response {
					Executor::std()
						.run(analysis.1.as_str(), &output.value)
						.map_err(|e| hc_error!("{}", e))?
				} else {
					false
				}
			};

			// Record in output map
			plugin_results.table.insert(
				analysis.0.clone(),
				PluginAnalysisResult {
					response,
					policy: analysis.1.clone(),
					passed,
				},
			);
		}

		ScoreTree::synthesize_plugin(&analysis_tree, &plugin_results)?
	};

	Ok(ScoringResults {
		results: plugin_results,
		score: {
			let mut score = Score::default();
			score.total = plugin_score_tree.score();
			score
		},
	})
}

fn decimal_truncate(score: f64) -> f64 {
	(score * 100.0).round() / 100.0
}

#[cfg(test)]
mod test {
	use super::*;

	//We use -1.0 values for parent nodes basically that get scored through recursion based on child nodes that have a score set and weight

	/*
		 risk - score: 0.77 [(0.56 * 0.333) + (0.87 * 0.667)]
		 |- practices - score: 0.56 [(1 * 0.56) + (0 * 0.44)]
		 |- review - score: 1
		 |- activity - score: 0
		 |- attacks - score: 0.87 [(0.74 * 0.5) + (1 * 0.5)]
		 |- high risk commits - score: 0.74 [(1 * 0.21) + (0 * 0.16) + (0 * 0.11) + (1 * 0.53)]
			 |- contributor trust - score: 1
			 |- code review - score: 0
			 |- entropy - score: 0
			 |- churn - score: 1
		 |- typosquatting - score: 1
	*/
	#[test]
	#[ignore = "test of tree scoring"]
	fn test_graph1() {
		let mut score_tree = ScoreTree::new("risk");
		let core = score_tree.root;
		let practices = score_tree.add_child(core, PRACTICES_PHASE, -1.0, 10.0);
		let _review = score_tree.add_child(practices, REVIEW_PHASE, 1.0, 5.0);
		let _activity = score_tree.add_child(practices, ACTIVITY_PHASE, 0.0, 4.0);
		let attacks = score_tree.add_child(core, ATTACKS_PHASE, -1.0, 20.0);
		let commits = score_tree.add_child(attacks, COMMITS_PHASE, -1.0, 5.0);
		let _trust = score_tree.add_child(commits, "trust", 1.0, 4.0);
		let _code_review = score_tree.add_child(commits, "code_review", 0.0, 3.0);
		let _entropy = score_tree.add_child(commits, ENTROPY_PHASE, 0.0, 2.0);
		let _churn = score_tree.add_child(commits, CHURN_PHASE, 1.0, 10.0);
		let _typo = score_tree.add_child(attacks, TYPO_PHASE, 1.0, 5.0);
		let final_score = score_tree.normalize().score();
		println!("final score {}", final_score);

		assert_eq!(0.76, final_score);
	}

	/*
		risk2 .4
		|- practices2 - weight: 10 (1*.4)
		|- review2 - score: 1, weight: 4 (1*.44)
		|- activity2 - score: 1, weight: 5(1*.56)
		|- attacks2 - weight: 15 (0*.6)
			|- code review - score: 0, weight: 6
			|- entropy - score: 0, weight: 7
	*/
	#[test]
	#[ignore = "test2 of tree scoring"]
	fn test_graph2() {
		let mut score_tree = ScoreTree::new("risk");
		let core = score_tree.root;
		let practices = score_tree.add_child(core, PRACTICES_PHASE, -1.0, 10.0);
		let _review = score_tree.add_child(practices, REVIEW_PHASE, 1.0, 4.0);
		let _activity = score_tree.add_child(practices, ACTIVITY_PHASE, 1.0, 5.0);
		let attacks = score_tree.add_child(core, ATTACKS_PHASE, -1.0, 15.0);
		let _code_review = score_tree.add_child(attacks, "code_review", 0.0, 6.0);
		let _entropy = score_tree.add_child(attacks, ENTROPY_PHASE, 0.0, 7.0);
		let final_score = score_tree.normalize().score();
		println!("final score {}", final_score);

		assert_eq!(0.4, final_score);
	}

	/*
		risk3 1
		|- practices3 - weight: 33 (1*.6875)
		|- review3 - score: 1, weight: 10  (1* .6666)
		|- activity3 - score: 1, weight: 5 (1* .3333)
		|- attacks3 - weight: 15 (1*.3125 )
			|- code review3 - score: 1, weight: 6 (1* .24)
			|- entropy3 - score: 1, weight: 19 (1* .76)
	*/
	#[test]
	#[ignore = "test3 of tree scoring"]
	fn test_graph3() {
		let mut score_tree = ScoreTree::new("risk");
		let core = score_tree.root;
		let practices = score_tree.add_child(core, PRACTICES_PHASE, -1.0, 33.0);
		let _review = score_tree.add_child(practices, REVIEW_PHASE, 1.0, 10.0);
		let _activity = score_tree.add_child(practices, ACTIVITY_PHASE, 1.0, 5.0);
		let attacks = score_tree.add_child(core, ATTACKS_PHASE, -1.0, 15.0);
		let _code_review = score_tree.add_child(attacks, "code_review", 1.0, 6.0);
		let _entropy = score_tree.add_child(attacks, ENTROPY_PHASE, 1.0, 15.0);
		let final_score = score_tree.normalize().score();
		println!("final score {}", final_score);

		assert_eq!(1.0, final_score);
	}

	/*
		Final score=(.2420) + (.1611) = .4031
		risk4 1 (.727*.333) + (.591*.2727)
		|- practices4 - weight: 40 .333
		|- review4 - score: 0, weight: 12  (0* .666 )
		|- activity4 - score: 1, weight: 6 (1* .333 )
		|- attacks4 - weight: 15 .591
			|- code review4 - score: 0, weight: 9 (0*0.409 )
			|- entropy4 - score: 1, weight: 13 (1*.591)
	*/
	#[test]
	#[ignore = "test4 of tree scoring"]
	fn test_graph4() {
		let mut score_tree = ScoreTree::new("risk");
		let core = score_tree.root;
		let practices = score_tree.add_child(core, PRACTICES_PHASE, -1.0, 40.0);
		let _review = score_tree.add_child(practices, REVIEW_PHASE, 0.0, 12.0);
		let _activity = score_tree.add_child(practices, ACTIVITY_PHASE, 1.0, 6.0);
		let attacks = score_tree.add_child(core, ATTACKS_PHASE, -1.0, 15.0);
		let _code_review = score_tree.add_child(attacks, "code_review", 0.0, 9.0);
		let _entropy = score_tree.add_child(attacks, ENTROPY_PHASE, 1.0, 13.0);
		let final_score = score_tree.normalize().score();
		println!("final score {}", final_score);

		assert_eq!(0.40, final_score);
	}
}
