// SPDX-License-Identifier: Apache-2.0

use crate::analysis::analysis::AnalysisOutcome;
use crate::analysis::analysis::AnalysisReport;
use crate::analysis::result::*;
use crate::analysis::AnalysisProvider;
use crate::error::Result;
use crate::hc_error;
use crate::report::Concern;
use crate::shell::Phase;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::default::Default;
use std::sync::Arc;

use petgraph::graph::node_index as n;
use petgraph::graph::NodeIndex;
use petgraph::prelude::Graph;
use petgraph::EdgeDirection::Outgoing;

pub const RISK_PHASE: &str = "risk";

pub const PRACTICES_PHASE: &str = "practices";
pub const REVIEW_PHASE: &str = "review";
pub const IDENTITY_PHASE: &str = "identity";
pub const BINARY_PHASE: &str = "binary";
pub const ACTIVITY_PHASE: &str = "activity";
pub const FUZZ_PHASE: &str = "fuzz";

pub const COMMITS_PHASE: &str = "high risk commits";
pub const TYPO_PHASE: &str = "typo";
pub const ATTACKS_PHASE: &str = "attacks";
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
pub struct AltAnalysisResults {
	pub table: HashMap<String, HCStoredResult>,
}
impl AltAnalysisResults {
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

#[allow(dead_code)]
#[derive(Debug, Default)]
pub struct AnalysisResults {
	pub activity: Option<HCStoredResult>,
	pub affiliation: Option<HCStoredResult>,
	pub binary: Option<HCStoredResult>,
	pub churn: Option<HCStoredResult>,
	pub entropy: Option<HCStoredResult>,
	pub identity: Option<HCStoredResult>,
	pub fuzz: Option<HCStoredResult>,
	pub review: Option<HCStoredResult>,
	pub typo: Option<HCStoredResult>,
	pub pull_request: Option<Result<Arc<AnalysisReport>>>,
}

#[allow(dead_code)]
#[derive(Debug, Default)]
pub struct Score {
	pub total: f64,
	pub activity: AnalysisOutcome,
	pub affiliation: AnalysisOutcome,
	pub binary: AnalysisOutcome,
	pub churn: AnalysisOutcome,
	pub entropy: AnalysisOutcome,
	pub identity: AnalysisOutcome,
	pub fuzz: AnalysisOutcome,
	pub review: AnalysisOutcome,
	pub typo: AnalysisOutcome,
	pub pull_request: AnalysisOutcome,
}

#[salsa::query_group(ScoringProviderStorage)]
pub trait ScoringProvider: AnalysisProvider {
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

//stores the score tree using petgraph
//the tree does not need to know what sections it is scoring
#[derive(Debug, Clone)]
pub struct ScoreTree {
	pub tree: Graph<ScoreTreeNode, f64>,
}

//stores the score tree using petgraph
//the tree does not need to know what sections it is scoring
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ScoreTreeNode {
	pub label: String,
	pub score: f64,
	pub weight: f64,
}

// returns score result for each phase based on phase name
// pass (a score of 0) or fail (a score of 1)
pub fn phase_outcome<P: AsRef<String>>(
	db: &dyn ScoringProvider,
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

pub fn update_phase(phase: &mut Phase, phase_name: &str) -> Result<()> {
	match phase.update(phase_name) {
		Ok(()) => Ok(()),
		_ => Err(hc_error!("failed to update {} phase.", phase_name)),
	}
}

//Scores phase and adds nodes and edges to tree
pub fn add_node_and_edge_with_score(
	score_result: impl AsRef<ScoreResult>,
	mut score_tree: ScoreTree,
	phase: &str,
	parent_node: NodeIndex<u32>,
) -> Result<ScoreTree> {
	let weight = score_result.as_ref().count as f64;
	let score_increment = score_result.as_ref().score as i64;

	//adding nodes/edges to the score tree
	let (child_node, score_tree_updated) =
		match add_tree_node(score_tree.clone(), phase, score_increment, weight) {
			Ok(results) => results,
			_ => {
				return Err(hc_error!(
					"failed to add score tree node for {} scoring.",
					phase
				))
			}
		};

	score_tree = add_tree_edge(score_tree_updated, child_node, parent_node);

	Ok(score_tree)
}

pub fn add_tree_node(
	mut score_tree: ScoreTree,
	phase: &str,
	score_increment: i64,
	weight: f64,
) -> Result<(NodeIndex, ScoreTree)> {
	let score_node = score_tree.tree.add_node(ScoreTreeNode {
		label: phase.to_string(),
		score: score_increment as f64,
		weight,
	});
	Ok((score_node, score_tree))
}

pub fn add_tree_edge(
	mut score_tree: ScoreTree,
	child_node: NodeIndex,
	parent_node: NodeIndex,
) -> ScoreTree {
	score_tree.tree.add_edge(parent_node, child_node, 0.0);
	score_tree
}

macro_rules! run_and_score_threshold_analysis {
	($res:ident, $p:ident, $tree: ident, $phase:ident, $a:expr, $w:expr, $spec:ident, $node:ident) => {{
		update_phase($p, $phase)?;
		let analysis_result =
			ThresholdPredicate::from_analysis(&$a, $spec.threshold, $spec.units, $spec.ordering);
		$res.table.insert($phase.to_owned(), analysis_result);
		let (an_score, outcome) = $res.table.get($phase).unwrap().score();
		let score_result = Arc::new(ScoreResult {
			count: $w,
			score: an_score,
			outcome,
		});
		let output = score_result.outcome.clone();
		match add_node_and_edge_with_score(score_result, $tree, $phase, $node) {
			Ok(score_tree_inc) => {
				$tree = score_tree_inc;
			}
			_ => return Err(hc_error!("failed to complete {} scoring.", $phase)),
		};
		output
	}};
}

pub fn score_results(phase: &mut Phase, db: &dyn ScoringProvider) -> Result<ScoringResults> {
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

	let mut results = AnalysisResults::default();
	let mut alt_results = AltAnalysisResults::default();
	let mut score = Score::default();
	let mut score_tree = ScoreTree { tree: Graph::new() };
	let root_node = score_tree.tree.add_node(ScoreTreeNode {
		label: RISK_PHASE.to_string(),
		score: -1.0,
		weight: 0.0,
	});
	/* PRACTICES NODE ADDITION */
	if db.practices_active() {
		let (practices_node, score_tree_updated) = match add_tree_node(
			score_tree.clone(),
			PRACTICES_PHASE,
			0,
			db.practices_weight() as f64,
		) {
			Ok(results) => results,
			_ => {
				return Err(hc_error!(
					"failed to add score tree node for {} scoring.",
					PRACTICES_PHASE
				))
			}
		};

		score_tree = add_tree_edge(score_tree_updated, practices_node, root_node);

		/*===NEW_PHASE===*/
		if db.activity_active() {
			let spec = ThresholdSpec {
				threshold: HCBasicValue::from(db.activity_week_count_threshold()),
				units: Some("weeks inactivity".to_owned()),
				ordering: Ordering::Less,
			};
			score.activity = run_and_score_threshold_analysis!(
				alt_results,
				phase,
				score_tree,
				ACTIVITY_PHASE,
				db.activity_analysis(),
				db.activity_weight(),
				spec,
				practices_node
			);
			results.activity = Some(alt_results.table.get(ACTIVITY_PHASE).unwrap().clone());
		}

		/*===REVIEW PHASE===*/
		if db.review_active() {
			let spec = ThresholdSpec {
				threshold: HCBasicValue::from(db.review_percent_threshold()),
				units: Some("% pull requests without review".to_owned()),
				ordering: Ordering::Less,
			};
			score.review = run_and_score_threshold_analysis!(
				alt_results,
				phase,
				score_tree,
				REVIEW_PHASE,
				db.review_analysis(),
				db.review_weight(),
				spec,
				practices_node
			);
			results.review = Some(alt_results.table.get(REVIEW_PHASE).unwrap().clone());
		}

		/*===BINARY PHASE===*/
		if db.binary_active() {
			let spec = ThresholdSpec {
				threshold: HCBasicValue::from(db.binary_count_threshold()),
				units: Some("binary files found".to_owned()),
				ordering: Ordering::Less,
			};
			score.binary = run_and_score_threshold_analysis!(
				alt_results,
				phase,
				score_tree,
				BINARY_PHASE,
				db.binary_analysis(),
				db.binary_weight(),
				spec,
				practices_node
			);
			results.binary = Some(alt_results.table.get(BINARY_PHASE).unwrap().clone());
		}

		/*===IDENTITY PHASE===*/
		if db.identity_active() {
			let spec = ThresholdSpec {
				threshold: HCBasicValue::from(db.identity_percent_threshold()),
				units: Some("% identity match".to_owned()),
				ordering: Ordering::Less,
			};
			score.identity = run_and_score_threshold_analysis!(
				alt_results,
				phase,
				score_tree,
				IDENTITY_PHASE,
				db.identity_analysis(),
				db.identity_weight(),
				spec,
				practices_node
			);
			results.identity = Some(alt_results.table.get(IDENTITY_PHASE).unwrap().clone());
		}

		/*===FUZZ PHASE===*/
		if db.fuzz_active() {
			let spec = ThresholdSpec {
				threshold: HCBasicValue::from(true),
				units: None,
				ordering: Ordering::Equal,
			};
			score.fuzz = run_and_score_threshold_analysis!(
				alt_results,
				phase,
				score_tree,
				FUZZ_PHASE,
				db.fuzz_analysis(),
				db.fuzz_weight(),
				spec,
				practices_node
			);
			results.fuzz = Some(alt_results.table.get(FUZZ_PHASE).unwrap().clone());
		}
	}

	/* ATTACKS NODE ADDITION */
	if db.attacks_active() {
		let (attacks_node, score_tree_updated) = match add_tree_node(
			score_tree.clone(),
			ATTACKS_PHASE,
			0,
			db.attacks_weight() as f64,
		) {
			Ok(results) => results,
			_ => {
				return Err(hc_error!(
					"failed to add score tree node for {} scoring.",
					ATTACKS_PHASE
				))
			}
		};

		score_tree = add_tree_edge(score_tree_updated, attacks_node, root_node);

		/*===TYPO PHASE===*/
		if db.typo_active() {
			let spec = ThresholdSpec {
				threshold: HCBasicValue::from(db.typo_count_threshold()),
				units: Some("possible typos".to_owned()),
				ordering: Ordering::Less,
			};
			score.typo = run_and_score_threshold_analysis!(
				alt_results,
				phase,
				score_tree,
				TYPO_PHASE,
				db.typo_analysis(),
				db.typo_weight(),
				spec,
				attacks_node
			);
			results.typo = Some(alt_results.table.get(TYPO_PHASE).unwrap().clone());
		}

		/*High risk commits node addition*/
		if db.commit_active() {
			let (commit_node, score_tree_updated) = match add_tree_node(
				score_tree.clone(),
				COMMITS_PHASE,
				0,
				db.commit_weight() as f64,
			) {
				Ok(results) => results,
				_ => {
					return Err(hc_error!(
						"failed to add score tree node for {} scoring.",
						COMMITS_PHASE
					))
				}
			};

			score_tree = add_tree_edge(score_tree_updated, commit_node, attacks_node);

			/*===NEW_PHASE===*/
			if db.affiliation_active() {
				let spec = ThresholdSpec {
					threshold: HCBasicValue::from(db.affiliation_count_threshold()),
					units: Some("affiliated".to_owned()),
					ordering: Ordering::Less,
				};
				score.affiliation = run_and_score_threshold_analysis!(
					alt_results,
					phase,
					score_tree,
					AFFILIATION_PHASE,
					db.affiliation_analysis(),
					db.affiliation_weight(),
					spec,
					commit_node
				);
				// This will be removed once results is deprecated in favor of alt_results
				results.affiliation =
					Some(alt_results.table.get(AFFILIATION_PHASE).unwrap().clone());
			}

			/*===NEW_PHASE===*/
			if db.churn_active() {
				let spec = ThresholdSpec {
					threshold: HCBasicValue::from(db.churn_percent_threshold()),
					units: Some("% over churn threshold".to_owned()),
					ordering: Ordering::Less,
				};
				score.churn = run_and_score_threshold_analysis!(
					alt_results,
					phase,
					score_tree,
					CHURN_PHASE,
					db.churn_analysis(),
					db.churn_weight(),
					spec,
					commit_node
				);
				results.churn = Some(alt_results.table.get(CHURN_PHASE).unwrap().clone());
			}

			/*===NEW_PHASE===*/
			if db.entropy_active() {
				let spec = ThresholdSpec {
					threshold: HCBasicValue::from(db.entropy_percent_threshold()),
					units: Some("% over entropy threshold".to_owned()),
					ordering: Ordering::Less,
				};
				score.entropy = run_and_score_threshold_analysis!(
					alt_results,
					phase,
					score_tree,
					ENTROPY_PHASE,
					db.entropy_analysis(),
					db.entropy_weight(),
					spec,
					commit_node
				);
				results.entropy = Some(alt_results.table.get(ENTROPY_PHASE).unwrap().clone());
			}
		}
	}

	let start_node: NodeIndex<u32> = n(0);
	score.total = score_nodes(start_node, score_tree.tree);

	Ok(ScoringResults { results, score })
}

fn score_nodes(node: NodeIndex, gr: Graph<ScoreTreeNode, f64>) -> f64 {
	//get all children to get full weight for node/branch level
	let mut children = gr.neighbors_directed(node, Outgoing).detach();

	let mut child_weight_sums = 0.0;
	let sums = gr.clone();
	let mut weights = children.clone();
	//add child weights together so we can get weight later
	while let Some(child) = weights.next_node(&sums) {
		child_weight_sums += sums[child].weight;
	}

	if child_weight_sums == 0.0 {
		//if node has no children return the node score
		return gr[node].score;
	}

	let mut score = 0.0;
	while let Some(child) = children.next_node(&gr) {
		let child_weight = gr[child].weight / child_weight_sums;

		//using recursion to get children node scores, this takes us to last node first in the end
		score += score_nodes(child, gr.clone()) * child_weight;
	}
	decimal_truncate(score)
}

fn decimal_truncate(score: f64) -> f64 {
	(score * 100.0).round() / 100.0
}

#[cfg(test)]
mod test {
	use super::*;
	use petgraph::Graph;

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
		let mut score_tree = ScoreTree { tree: Graph::new() };
		let core = score_tree.tree.add_node(ScoreTreeNode {
			label: "risk".to_string(),
			score: -1.0,
			weight: -1.0,
		});
		let practices = score_tree.tree.add_node(ScoreTreeNode {
			label: PRACTICES_PHASE.to_string(),
			score: -1.0,
			weight: 10.0,
		});
		let review = score_tree.tree.add_node(ScoreTreeNode {
			label: REVIEW_PHASE.to_string(),
			score: 1.0,
			weight: 5.0,
		});
		let activity = score_tree.tree.add_node(ScoreTreeNode {
			label: ACTIVITY_PHASE.to_string(),
			score: 0.0,
			weight: 4.0,
		});
		let attacks = score_tree.tree.add_node(ScoreTreeNode {
			label: ATTACKS_PHASE.to_string(),
			score: -1.0,
			weight: 20.0,
		});
		let commits = score_tree.tree.add_node(ScoreTreeNode {
			label: COMMITS_PHASE.to_string(),
			score: -1.0,
			weight: 5.0,
		});
		let trust = score_tree.tree.add_node(ScoreTreeNode {
			label: "trust".to_string(),
			score: 1.0,
			weight: 4.0,
		});
		let code_review = score_tree.tree.add_node(ScoreTreeNode {
			label: "code review".to_string(),
			score: 0.0,
			weight: 3.0,
		});
		let entropy = score_tree.tree.add_node(ScoreTreeNode {
			label: ENTROPY_PHASE.to_string(),
			score: 0.0,
			weight: 2.0,
		});
		let churn = score_tree.tree.add_node(ScoreTreeNode {
			label: CHURN_PHASE.to_string(),
			score: 1.0,
			weight: 10.0,
		});
		let typo = score_tree.tree.add_node(ScoreTreeNode {
			label: TYPO_PHASE.to_string(),
			score: 1.0,
			weight: 5.0,
		});
		//edge weights are not used
		score_tree.tree.add_edge(core, practices, 0.0);
		score_tree.tree.add_edge(core, attacks, 0.0);

		score_tree.tree.add_edge(practices, review, 0.0);
		score_tree.tree.add_edge(practices, activity, 0.0);

		score_tree.tree.add_edge(attacks, commits, 0.0);
		score_tree.tree.add_edge(attacks, typo, 0.0);

		score_tree.tree.add_edge(commits, trust, 0.0);
		score_tree.tree.add_edge(commits, code_review, 0.0);
		score_tree.tree.add_edge(commits, entropy, 0.0);
		score_tree.tree.add_edge(commits, churn, 0.0);

		let final_score = score_nodes(core, score_tree.tree);
		println!("final score {}", final_score);

		assert_eq!(0.77, final_score);
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
		let mut score_tree = ScoreTree { tree: Graph::new() };
		let core = score_tree.tree.add_node(ScoreTreeNode {
			label: "risk".to_string(),
			score: -1.0,
			weight: -1.0,
		});
		let practices = score_tree.tree.add_node(ScoreTreeNode {
			label: PRACTICES_PHASE.to_string(),
			score: -1.0,
			weight: 10.0,
		});
		let review = score_tree.tree.add_node(ScoreTreeNode {
			label: REVIEW_PHASE.to_string(),
			score: 1.0,
			weight: 4.0,
		});
		let activity = score_tree.tree.add_node(ScoreTreeNode {
			label: ACTIVITY_PHASE.to_string(),
			score: 1.0,
			weight: 5.0,
		});
		let attacks = score_tree.tree.add_node(ScoreTreeNode {
			label: ATTACKS_PHASE.to_string(),
			score: -1.0,
			weight: 15.0,
		});
		let code_review = score_tree.tree.add_node(ScoreTreeNode {
			label: "code review".to_string(),
			score: 0.0,
			weight: 6.0,
		});
		let entropy = score_tree.tree.add_node(ScoreTreeNode {
			label: ENTROPY_PHASE.to_string(),
			score: 0.0,
			weight: 7.0,
		});
		//edge weights are not used
		score_tree.tree.add_edge(core, practices, 0.0);
		score_tree.tree.add_edge(core, attacks, 0.0);

		score_tree.tree.add_edge(practices, review, 0.0);
		score_tree.tree.add_edge(practices, activity, 0.0);

		score_tree.tree.add_edge(attacks, code_review, 0.0);
		score_tree.tree.add_edge(attacks, entropy, 0.0);

		let final_score = score_nodes(core, score_tree.tree);
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
		let mut score_tree = ScoreTree { tree: Graph::new() };
		let core = score_tree.tree.add_node(ScoreTreeNode {
			label: "risk".to_string(),
			score: -1.0,
			weight: -1.0,
		});
		let practices = score_tree.tree.add_node(ScoreTreeNode {
			label: PRACTICES_PHASE.to_string(),
			score: -1.0,
			weight: 33.0,
		});
		let review = score_tree.tree.add_node(ScoreTreeNode {
			label: REVIEW_PHASE.to_string(),
			score: 1.0,
			weight: 10.0,
		});
		let activity = score_tree.tree.add_node(ScoreTreeNode {
			label: ACTIVITY_PHASE.to_string(),
			score: 1.0,
			weight: 5.0,
		});
		let attacks = score_tree.tree.add_node(ScoreTreeNode {
			label: ATTACKS_PHASE.to_string(),
			score: -1.0,
			weight: 15.0,
		});
		let code_review = score_tree.tree.add_node(ScoreTreeNode {
			label: "code review".to_string(),
			score: 1.0,
			weight: 6.0,
		});
		let entropy = score_tree.tree.add_node(ScoreTreeNode {
			label: ENTROPY_PHASE.to_string(),
			score: 1.0,
			weight: 15.0,
		});
		//edge weights are not used
		score_tree.tree.add_edge(core, practices, 0.0);
		score_tree.tree.add_edge(core, attacks, 0.0);

		score_tree.tree.add_edge(practices, review, 0.0);
		score_tree.tree.add_edge(practices, activity, 0.0);

		score_tree.tree.add_edge(attacks, code_review, 0.0);
		score_tree.tree.add_edge(attacks, entropy, 0.0);

		let final_score = score_nodes(core, score_tree.tree);
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
		let mut score_tree = ScoreTree { tree: Graph::new() };
		let core = score_tree.tree.add_node(ScoreTreeNode {
			label: "risk".to_string(),
			score: -1.0,
			weight: -1.0,
		});
		let practices = score_tree.tree.add_node(ScoreTreeNode {
			label: PRACTICES_PHASE.to_string(),
			score: -1.0,
			weight: 40.0,
		});
		let review = score_tree.tree.add_node(ScoreTreeNode {
			label: REVIEW_PHASE.to_string(),
			score: 0.0,
			weight: 12.0,
		});
		let activity = score_tree.tree.add_node(ScoreTreeNode {
			label: ACTIVITY_PHASE.to_string(),
			score: 1.0,
			weight: 6.0,
		});
		let attacks = score_tree.tree.add_node(ScoreTreeNode {
			label: ATTACKS_PHASE.to_string(),
			score: -1.0,
			weight: 15.0,
		});
		let code_review = score_tree.tree.add_node(ScoreTreeNode {
			label: "code review".to_string(),
			score: 0.0,
			weight: 9.0,
		});
		let entropy = score_tree.tree.add_node(ScoreTreeNode {
			label: ENTROPY_PHASE.to_string(),
			score: 1.0,
			weight: 13.0,
		});
		//edge weights are not used
		score_tree.tree.add_edge(core, practices, 0.0);
		score_tree.tree.add_edge(core, attacks, 0.0);

		score_tree.tree.add_edge(practices, review, 0.0);
		score_tree.tree.add_edge(practices, activity, 0.0);

		score_tree.tree.add_edge(attacks, code_review, 0.0);
		score_tree.tree.add_edge(attacks, entropy, 0.0);

		let final_score = score_nodes(core, score_tree.tree);
		println!("final score {}", final_score);

		assert_eq!(0.40, final_score);
	}
}
