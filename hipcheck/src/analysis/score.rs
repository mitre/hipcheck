// SPDX-License-Identifier: Apache-2.0

use crate::{
	analysis::{result::*, AnalysisOutcome, AnalysisProvider},
	config::{
		visit_leaves, Analysis, AnalysisTree, WeightTreeProvider, DEFAULT_QUERY, MITRE_PUBLISHER,
	},
	engine::HcEngine,
	error::Result,
	hc_error,
	plugin::QueryResult,
	report::Concern,
	shell::spinner_phase::SpinnerPhase,
};
use indextree::{Arena, NodeId};
use num_traits::identities::Zero;
use serde_json::Value;
use std::{collections::HashMap, default::Default, sync::Arc};

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
	pub results: AnalysisResults,
	pub score: Score,
}

#[derive(Debug, Clone)]
pub struct HCStoredResult {
	pub result: Result<Arc<Predicate>>,
	pub concerns: Vec<Concern>,
}
impl HCStoredResult {
	// Score the analysis by invoking predicate's impl of `pass()`. Errored
	// analyses treated as failures.
	// @FollowUp - remove AnalysisOutcome once scoring refactor complete
	#[allow(unused)]
	pub fn score(&self) -> (u64, AnalysisOutcome) {
		match &self.result {
			Err(e) => (1, AnalysisOutcome::Error(e.clone())),
			Ok(pred) => {
				let passed = match pred.pass() {
					Err(err) => {
						return (1, AnalysisOutcome::Error(err));
					}
					Ok(p) => p,
				};
				let msg = pred.to_string();
				let outcome = if passed {
					AnalysisOutcome::Pass(msg)
				} else {
					AnalysisOutcome::Fail(msg)
				};
				let score = (!passed) as u64;
				(score, outcome)
			}
		}
	}
}

#[derive(Debug, Default)]
pub struct PluginAnalysisResults {
	pub table: HashMap<Analysis, Result<QueryResult>>,
}

#[derive(Debug, Default)]
pub struct AnalysisResults {
	pub table: HashMap<String, HCStoredResult>,
}

impl AnalysisResults {
	/// Get all results from plugin-based analyses.
	pub fn plugin_results(&self) -> impl Iterator<Item = (String, &HCStoredResult)> {
		self.table.iter().filter_map(|(name, result)| {
			if [
				REVIEW_PHASE,
				IDENTITY_PHASE,
				BINARY_PHASE,
				ACTIVITY_PHASE,
				FUZZ_PHASE,
				TYPO_PHASE,
				AFFILIATION_PHASE,
				CHURN_PHASE,
				ENTROPY_PHASE,
			]
			// Horrifying conversion, but necessary.
			.contains(&&(*name).as_ref())
			{
				None
			} else {
				Some((name.to_owned(), result))
			}
		})
	}

	#[allow(unused)]
	pub fn add(
		&mut self,
		key: &str,
		result: Result<Arc<Predicate>>,
		concerns: Vec<Concern>,
	) -> Result<()> {
		if self.table.contains_key(key) {
			return Err(hc_error!(
				"analysis results table already contains key '{key}'"
			));
		}
		let result_struct = HCStoredResult { result, concerns };
		self.table.insert(key.to_owned(), result_struct);
		Ok(())
	}
}

#[derive(Debug, Default)]
pub struct Score {
	pub total: f64,
	#[allow(unused)]
	pub activity: AnalysisOutcome,
	#[allow(unused)]
	pub affiliation: AnalysisOutcome,
	#[allow(unused)]
	pub binary: AnalysisOutcome,
	#[allow(unused)]
	pub churn: AnalysisOutcome,
	#[allow(unused)]
	pub entropy: AnalysisOutcome,
	#[allow(unused)]
	pub identity: AnalysisOutcome,
	#[allow(unused)]
	pub fuzz: AnalysisOutcome,
	#[allow(unused)]
	pub review: AnalysisOutcome,
	#[allow(unused)]
	pub typo: AnalysisOutcome,
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
	/// Returns result of phase outcome and scoring
	fn phase_outcome(&self, phase_name: Arc<String>) -> Result<Arc<ScoreResult>>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoreResult {
	pub count: u64,
	pub score: u64,
	pub outcome: AnalysisOutcome,
}

//impl Eq for ScoreResult {}

impl Default for ScoreResult {
	fn default() -> ScoreResult {
		ScoreResult {
			count: 0,
			score: 0,
			outcome: AnalysisOutcome::Skipped,
		}
	}
}

#[allow(unused)]
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
	#[allow(unused)]
	pub fn new(root_label: &str) -> Self {
		let mut tree = Arena::<ScoreTreeNode>::new();
		let root = tree.new_node(ScoreTreeNode {
			label: root_label.to_owned(),
			score: -1.0,
			weight: 1.0,
		});
		ScoreTree { tree, root }
	}

	#[allow(unused)]
	pub fn add_child(&mut self, under: NodeId, label: &str, score: f64, weight: f64) -> NodeId {
		let child = self.tree.new_node(ScoreTreeNode {
			label: label.to_owned(),
			score,
			weight,
		});
		under.append(child, &mut self.tree);
		child
	}

	#[allow(unused)]
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

	// Given an analysis tree and set of analysis results, produce an AltScoreTree by creating
	// ScoreTreeNode objects for each analysis that was not skipped, and composing them into
	// a tree structure matching that of the WeightTree
	#[allow(unused)]
	pub fn synthesize(analysis_tree: &AnalysisTree, scores: &AnalysisResults) -> Result<Self> {
		use indextree::NodeEdge::*;
		let mut tree = Arena::<ScoreTreeNode>::new();
		let analysis_root = analysis_tree.root;
		let score_root = tree.new_node(
			analysis_tree
				.tree
				.get(analysis_root)
				.ok_or(hc_error!("AnalysisTree root not in tree, invalid state"))?
				.get()
				.augment(scores)?,
		);

		let mut scope: Vec<NodeId> = vec![score_root];
		for edge in analysis_root.traverse(&analysis_tree.tree) {
			match edge {
				Start(n) => {
					let curr_node = tree.new_node(
						analysis_tree
							.tree
							.get(n)
							.ok_or(hc_error!("AnalysisTree node not in tree, invalid state"))?
							.get()
							.augment(scores)?,
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

// returns score result for each phase based on phase name
// pass (a score of 0) or fail (a score of 1)
pub fn phase_outcome<P: AsRef<String>>(
	_db: &dyn ScoringProvider,
	phase_name: P,
) -> Result<Arc<ScoreResult>> {
	match phase_name.as_ref().as_str() {
		ACTIVITY_PHASE => Err(hc_error!(
			"activity analysis does not use this infrastructure"
		)),
		AFFILIATION_PHASE => Err(hc_error!(
			"affiliation analysis does not use this infrastructure"
		)),
		BINARY_PHASE => Err(hc_error!(
			"binary analysis does not use this infrastructure"
		)),
		CHURN_PHASE => Err(hc_error!("churn analysis does not use this infrastructure")),
		ENTROPY_PHASE => Err(hc_error!(
			"entropy analysis does not use this infrastructure"
		)),
		IDENTITY_PHASE => Err(hc_error!(
			"identity analysis does not use this infrastructure"
		)),
		FUZZ_PHASE => Err(hc_error!("fuzz analysis does not use this infrastructure")),
		REVIEW_PHASE => Err(hc_error!(
			"review analysis does not use this infrastructure"
		)),
		TYPO_PHASE => Err(hc_error!("typo analysis does not use this infrastructure")),
		_ => Err(hc_error!(
			"failed to complete {} analysis.",
			phase_name.as_ref()
		)),
	}
}

#[allow(unused)]
macro_rules! run_and_score_threshold_analysis {
	($res:ident, $p:ident, $phase:ident, $a:expr, $spec:ident) => {{
		$p.update_status($phase);
		let analysis_result =
			ThresholdPredicate::from_analysis(&$a, $spec.threshold, $spec.units, $spec.ordering);
		$res.table.insert($phase.to_owned(), analysis_result);
		let (_an_score, outcome) = $res.table.get($phase).unwrap().score();
		outcome
	}};
}

fn wrapped_query(
	db: &dyn ScoringProvider,
	publisher: String,
	plugin: String,
	query: String,
	key: Value,
) -> Result<QueryResult> {
	if publisher == *MITRE_PUBLISHER {
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
	/*
	Scoring should be performed by the construction of a "score tree" where scores are the
	nodes and weights are the edges. The leaves are the analyses themselves, which either
	pass (a score of 0) or fail (a score of 1). These are then combined with the other
	children of their parent according to their weights, repeating until the final score is
	reached.
	generate the tree
	traverse and score using recursion of node children
	*/
	// Values set with -1.0 are reseved for parent nodes whose score comes always from children nodes with a score set by hc_analysis algorithms

	let analysis_tree = db.normalized_analysis_tree()?;
	let target = db.target();
	let target_json = serde_json::to_value(target.as_ref())?;
	let mut plug_results = PluginAnalysisResults::default();

	// @FollowUp - remove this once we implement policy expr calculation
	let results = AnalysisResults::default();

	let mut score = Score::default();

	// RFD4 analysis style - get all "leaf" analyses and call through plugin architecture
	let analyses = analysis_tree.get_analyses();
	for a in analyses {
		let result = db.wrapped_query(
			a.publisher.clone(),
			a.plugin.clone(),
			a.query.clone(),
			target_json.clone(),
		);
		plug_results.table.insert(a.clone(), result);
	}

	/*
	/* PRACTICES NODE ADDITION */
	/*===NEW_PHASE===*/
	let spec = ThresholdSpec {
		threshold: HCBasicValue::from(db.activity_week_count_threshold()),
		units: Some("weeks inactivity".to_owned()),
		ordering: Ordering::Less,
	};
	score.activity = run_and_score_threshold_analysis!(
		results,
		phase,
		ACTIVITY_PHASE,
		db.activity_analysis(),
		spec
	);

	/*===REVIEW PHASE===*/
	let spec = ThresholdSpec {
		threshold: HCBasicValue::from(db.review_percent_threshold()),
		units: Some("% pull requests without review".to_owned()),
		ordering: Ordering::Less,
	};
	score.review =
		run_and_score_threshold_analysis!(results, phase, REVIEW_PHASE, db.review_analysis(), spec);

	/*===BINARY PHASE===*/
	let spec = ThresholdSpec {
		threshold: HCBasicValue::from(db.binary_count_threshold()),
		units: Some("binary files found".to_owned()),
		ordering: Ordering::Less,
	};
	score.binary =
		run_and_score_threshold_analysis!(results, phase, BINARY_PHASE, db.binary_analysis(), spec);

	/*===IDENTITY PHASE===*/
	let spec = ThresholdSpec {
		threshold: HCBasicValue::from(db.identity_percent_threshold()),
		units: Some("% identity match".to_owned()),
		ordering: Ordering::Less,
	};
	score.identity = run_and_score_threshold_analysis!(
		results,
		phase,
		IDENTITY_PHASE,
		db.identity_analysis(),
		spec
	);

	/*===FUZZ PHASE===*/
	let spec = ThresholdSpec {
		threshold: HCBasicValue::from(true),
		units: None,
		ordering: Ordering::Equal,
	};
	score.fuzz =
		run_and_score_threshold_analysis!(results, phase, FUZZ_PHASE, db.fuzz_analysis(), spec);

	/* ATTACKS NODE ADDITION */
	/*===TYPO PHASE===*/
	let spec = ThresholdSpec {
		threshold: HCBasicValue::from(db.typo_count_threshold()),
		units: Some("possible typos".to_owned()),
		ordering: Ordering::Less,
	};
	score.typo =
		run_and_score_threshold_analysis!(results, phase, TYPO_PHASE, db.typo_analysis(), spec);

	/*High risk commits node addition*/
	/*===NEW_PHASE===*/
	let spec = ThresholdSpec {
		threshold: HCBasicValue::from(db.affiliation_count_threshold()),
		units: Some("affiliated".to_owned()),
		ordering: Ordering::Less,
	};
	score.affiliation = run_and_score_threshold_analysis!(
		results,
		phase,
		AFFILIATION_PHASE,
		db.affiliation_analysis(),
		spec
	);

	/*===NEW_PHASE===*/
	let spec = ThresholdSpec {
		threshold: HCBasicValue::from(db.churn_percent_threshold()),
		units: Some("% over churn threshold".to_owned()),
		ordering: Ordering::Less,
	};
	score.churn =
		run_and_score_threshold_analysis!(results, phase, CHURN_PHASE, db.churn_analysis(), spec);

	/*===NEW_PHASE===*/
	let spec = ThresholdSpec {
		threshold: HCBasicValue::from(db.entropy_percent_threshold()),
		units: Some("% over entropy threshold".to_owned()),
		ordering: Ordering::Less,
	};
	score.entropy = run_and_score_threshold_analysis!(
		results,
		phase,
		ENTROPY_PHASE,
		db.entropy_analysis(),
		spec
	);
	*/

	let plug_score_tree = ScoreTree::synthesize_plugin(&analysis_tree, &plug_results)?;
	// let alt_score_tree = ScoreTree::synthesize(&analysis_tree, &results)?;

	score.total = plug_score_tree.score();

	Ok(ScoringResults { results, score })
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
