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
use std::default::Default;
use std::rc::Rc;

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
pub const PR_AFFILIATION_PHASE: &str = "pull request affiliation";
pub const PR_CONTRIBUTOR_TRUST_PHASE: &str = "pull request contributor trust";
pub const PR_MODULE_CONTRIBUTORS_PHASE: &str = "pull request module contributors";

#[derive(Debug, Default)]
pub struct ScoringResults {
	pub results: AnalysisResults,
	pub score: Score,
}

#[allow(dead_code)]
#[derive(Debug, Default)]
pub struct AnalysisResults {
	pub activity: Option<(Result<Rc<ThresholdPredicate>>, Vec<Concern>)>,
	pub affiliation: Option<Result<Rc<AnalysisReport>>>,
	pub binary: Option<Result<Rc<AnalysisReport>>>,
	pub churn: Option<Result<Rc<AnalysisReport>>>,
	pub entropy: Option<Result<Rc<AnalysisReport>>>,
	pub identity: Option<Result<Rc<AnalysisReport>>>,
	pub fuzz: Option<Result<Rc<AnalysisReport>>>,
	pub review: Option<Result<Rc<AnalysisReport>>>,
	pub typo: Option<Result<Rc<AnalysisReport>>>,
	pub pull_request: Option<Result<Rc<AnalysisReport>>>,
	pub pr_affiliation: Option<Result<Rc<AnalysisReport>>>,
	pub pr_contributor_trust: Option<Result<Rc<AnalysisReport>>>,
	pub pr_module_contributors: Option<Result<Rc<AnalysisReport>>>,
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
	pub pr_affiliation: AnalysisOutcome,
	pub pr_contributor_trust: AnalysisOutcome,
	pub pr_module_contributors: AnalysisOutcome,
}

#[salsa::query_group(ScoringProviderStorage)]
pub trait ScoringProvider: AnalysisProvider {
	/// Returns result of phase outcome and scoring
	fn phase_outcome(&self, phase_name: Rc<String>) -> Result<Rc<ScoreResult>>;
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
) -> Result<Rc<ScoreResult>> {
	match phase_name.as_ref().as_str() {
		ACTIVITY_PHASE => Err(hc_error!(
			"activity analysis does not use this infrastructure"
		)),
		AFFILIATION_PHASE => match &db.affiliation_analysis().unwrap().as_ref() {
			AnalysisReport::None {
				outcome: AnalysisOutcome::Skipped,
			} => Ok(Rc::new(ScoreResult::default())),
			AnalysisReport::None {
				outcome: AnalysisOutcome::Error(msg),
			} => Ok(Rc::new(ScoreResult {
				count: 0,
				score: 0,
				outcome: AnalysisOutcome::Error(msg.clone()),
			})),
			AnalysisReport::Affiliation {
				outcome: AnalysisOutcome::Pass(msg),
				..
			} => Ok(Rc::new(ScoreResult {
				count: db.affiliation_weight(),
				score: 0,
				outcome: AnalysisOutcome::Pass(msg.to_string()),
			})),
			AnalysisReport::Affiliation {
				outcome: AnalysisOutcome::Fail(msg),
				..
			} => Ok(Rc::new(ScoreResult {
				count: db.affiliation_weight(),
				score: 1,
				outcome: AnalysisOutcome::Fail(msg.to_string()),
			})),
			_ => Err(hc_error!("phase name does not match analysis")),
		},

		BINARY_PHASE => match &db.binary_analysis().unwrap().as_ref() {
			AnalysisReport::None {
				outcome: AnalysisOutcome::Skipped,
			} => Ok(Rc::new(ScoreResult::default())),
			AnalysisReport::None {
				outcome: AnalysisOutcome::Error(msg),
			} => Ok(Rc::new(ScoreResult {
				count: 0,
				score: 0,
				outcome: AnalysisOutcome::Error(msg.clone()),
			})),
			AnalysisReport::Binary {
				outcome: AnalysisOutcome::Pass(msg),
				..
			} => Ok(Rc::new(ScoreResult {
				count: db.binary_weight(),
				score: 0,
				outcome: AnalysisOutcome::Pass(msg.to_string()),
			})),
			AnalysisReport::Binary {
				outcome: AnalysisOutcome::Fail(msg),
				..
			} => Ok(Rc::new(ScoreResult {
				count: db.binary_weight(),
				score: 1,
				outcome: AnalysisOutcome::Fail(msg.to_string()),
			})),
			_ => Err(hc_error!("phase name does not match analysis")),
		},

		CHURN_PHASE => match &db.churn_analysis().unwrap().as_ref() {
			AnalysisReport::None {
				outcome: AnalysisOutcome::Skipped,
			} => Ok(Rc::new(ScoreResult::default())),
			AnalysisReport::None {
				outcome: AnalysisOutcome::Error(msg),
			} => Ok(Rc::new(ScoreResult {
				count: 0,
				score: 0,
				outcome: AnalysisOutcome::Error(msg.clone()),
			})),
			AnalysisReport::Churn {
				outcome: AnalysisOutcome::Pass(msg),
				..
			} => Ok(Rc::new(ScoreResult {
				count: db.churn_weight(),
				score: 0,
				outcome: AnalysisOutcome::Pass(msg.to_string()),
			})),
			AnalysisReport::Churn {
				outcome: AnalysisOutcome::Fail(msg),
				..
			} => Ok(Rc::new(ScoreResult {
				count: db.churn_weight(),
				score: 1,
				outcome: AnalysisOutcome::Fail(msg.to_string()),
			})),
			_ => Err(hc_error!("phase name does not match analysis")),
		},

		ENTROPY_PHASE => match &db.entropy_analysis().unwrap().as_ref() {
			AnalysisReport::None {
				outcome: AnalysisOutcome::Skipped,
			} => Ok(Rc::new(ScoreResult::default())),
			AnalysisReport::None {
				outcome: AnalysisOutcome::Error(msg),
			} => Ok(Rc::new(ScoreResult {
				count: 0,
				score: 0,
				outcome: AnalysisOutcome::Error(msg.clone()),
			})),
			AnalysisReport::Entropy {
				outcome: AnalysisOutcome::Pass(msg),
				..
			} => Ok(Rc::new(ScoreResult {
				count: db.entropy_weight(),
				score: 0,
				outcome: AnalysisOutcome::Pass(msg.to_string()),
			})),
			AnalysisReport::Entropy {
				outcome: AnalysisOutcome::Fail(msg),
				..
			} => Ok(Rc::new(ScoreResult {
				count: db.entropy_weight(),
				score: 1,
				outcome: AnalysisOutcome::Fail(msg.to_string()),
			})),
			_ => Err(hc_error!("phase name does not match analysis")),
		},

		IDENTITY_PHASE => match &db.identity_analysis().unwrap().as_ref() {
			AnalysisReport::None {
				outcome: AnalysisOutcome::Skipped,
			} => Ok(Rc::new(ScoreResult::default())),
			AnalysisReport::None {
				outcome: AnalysisOutcome::Error(msg),
			} => Ok(Rc::new(ScoreResult {
				count: 0,
				score: 0,
				outcome: AnalysisOutcome::Error(msg.clone()),
			})),
			AnalysisReport::Identity {
				outcome: AnalysisOutcome::Pass(msg),
				..
			} => Ok(Rc::new(ScoreResult {
				count: db.identity_weight(),
				score: 0,
				outcome: AnalysisOutcome::Pass(msg.to_string()),
			})),
			AnalysisReport::Identity {
				outcome: AnalysisOutcome::Fail(msg),
				..
			} => Ok(Rc::new(ScoreResult {
				count: db.identity_weight(),
				score: 1,
				outcome: AnalysisOutcome::Fail(msg.to_string()),
			})),
			_ => Err(hc_error!("phase name does not match analysis")),
		},

		FUZZ_PHASE => match &db.fuzz_analysis().unwrap().as_ref() {
			AnalysisReport::None {
				outcome: AnalysisOutcome::Skipped,
			} => Ok(Rc::new(ScoreResult::default())),
			AnalysisReport::None {
				outcome: AnalysisOutcome::Error(msg),
			} => Ok(Rc::new(ScoreResult {
				count: 0,
				score: 0,
				outcome: AnalysisOutcome::Error(msg.clone()),
			})),
			AnalysisReport::Fuzz {
				outcome: AnalysisOutcome::Pass(msg),
				..
			} => Ok(Rc::new(ScoreResult {
				count: db.fuzz_weight(),
				score: 0,
				outcome: AnalysisOutcome::Pass(msg.to_string()),
			})),
			AnalysisReport::Fuzz {
				outcome: AnalysisOutcome::Fail(msg),
				..
			} => Ok(Rc::new(ScoreResult {
				count: db.fuzz_weight(),
				score: 1,
				outcome: AnalysisOutcome::Fail(msg.to_string()),
			})),
			_ => Err(hc_error!("phase name does not match analysis")),
		},

		REVIEW_PHASE => match &db.review_analysis().unwrap().as_ref() {
			AnalysisReport::None {
				outcome: AnalysisOutcome::Skipped,
			} => Ok(Rc::new(ScoreResult::default())),
			AnalysisReport::None {
				outcome: AnalysisOutcome::Error(msg),
			} => Ok(Rc::new(ScoreResult {
				count: 0,
				score: 0,
				outcome: AnalysisOutcome::Error(msg.clone()),
			})),
			AnalysisReport::Review {
				outcome: AnalysisOutcome::Pass(msg),
				..
			} => Ok(Rc::new(ScoreResult {
				count: db.review_weight(),
				score: 0,
				outcome: AnalysisOutcome::Pass(msg.to_string()),
			})),
			AnalysisReport::Review {
				outcome: AnalysisOutcome::Fail(msg),
				..
			} => Ok(Rc::new(ScoreResult {
				count: db.review_weight(),
				score: 1,
				outcome: AnalysisOutcome::Fail(msg.to_string()),
			})),
			_ => Err(hc_error!("phase name does not match analysis")),
		},

		TYPO_PHASE => match &db.typo_analysis().unwrap().as_ref() {
			AnalysisReport::None {
				outcome: AnalysisOutcome::Skipped,
			} => Ok(Rc::new(ScoreResult::default())),
			AnalysisReport::None {
				outcome: AnalysisOutcome::Error(msg),
			} => Ok(Rc::new(ScoreResult {
				count: 0,
				score: 0,
				outcome: AnalysisOutcome::Error(msg.clone()),
			})),
			AnalysisReport::Typo {
				outcome: AnalysisOutcome::Pass(msg),
				..
			} => Ok(Rc::new(ScoreResult {
				count: db.typo_weight(),
				score: 0,
				outcome: AnalysisOutcome::Pass(msg.to_string()),
			})),
			AnalysisReport::Typo {
				outcome: AnalysisOutcome::Fail(msg),
				..
			} => Ok(Rc::new(ScoreResult {
				count: db.typo_weight(),
				score: 1,
				outcome: AnalysisOutcome::Fail(msg.to_string()),
			})),
			_ => Err(hc_error!("phase name does not match analysis")),
		},

		PR_AFFILIATION_PHASE => match &db.pr_affiliation_analysis().unwrap().as_ref() {
			AnalysisReport::None {
				outcome: AnalysisOutcome::Skipped,
			} => Ok(Rc::new(ScoreResult::default())),
			AnalysisReport::None {
				outcome: AnalysisOutcome::Error(msg),
			} => Ok(Rc::new(ScoreResult {
				count: 0,
				score: 0,
				outcome: AnalysisOutcome::Error(msg.clone()),
			})),
			AnalysisReport::PrAffiliation {
				outcome: AnalysisOutcome::Pass(msg),
				..
			} => Ok(Rc::new(ScoreResult {
				count: db.pr_affiliation_weight(),
				score: 0,
				outcome: AnalysisOutcome::Pass(msg.to_string()),
			})),
			AnalysisReport::PrAffiliation {
				outcome: AnalysisOutcome::Fail(msg),
				..
			} => Ok(Rc::new(ScoreResult {
				count: db.pr_affiliation_weight(),
				score: 1,
				outcome: AnalysisOutcome::Fail(msg.to_string()),
			})),
			_ => Err(hc_error!("phase name does not match analysis")),
		},

		PR_CONTRIBUTOR_TRUST_PHASE => match &db.pr_contributor_trust_analysis().unwrap().as_ref() {
			AnalysisReport::None {
				outcome: AnalysisOutcome::Skipped,
			} => Ok(Rc::new(ScoreResult::default())),
			AnalysisReport::None {
				outcome: AnalysisOutcome::Error(msg),
			} => Ok(Rc::new(ScoreResult {
				count: 0,
				score: 0,
				outcome: AnalysisOutcome::Error(msg.clone()),
			})),
			AnalysisReport::PrContributorTrust {
				outcome: AnalysisOutcome::Pass(msg),
				..
			} => Ok(Rc::new(ScoreResult {
				count: db.contributor_trust_weight(),
				score: 0,
				outcome: AnalysisOutcome::Pass(msg.to_string()),
			})),
			AnalysisReport::PrContributorTrust {
				outcome: AnalysisOutcome::Fail(msg),
				..
			} => Ok(Rc::new(ScoreResult {
				count: db.contributor_trust_weight(),
				score: 1,
				outcome: AnalysisOutcome::Fail(msg.to_string()),
			})),
			_ => Err(hc_error!("phase name does not match analysis")),
		},

		PR_MODULE_CONTRIBUTORS_PHASE => {
			match &db.pr_module_contributors_analysis().unwrap().as_ref() {
				AnalysisReport::None {
					outcome: AnalysisOutcome::Skipped,
				} => Ok(Rc::new(ScoreResult::default())),
				AnalysisReport::None {
					outcome: AnalysisOutcome::Error(msg),
				} => Ok(Rc::new(ScoreResult {
					count: 0,
					score: 0,
					outcome: AnalysisOutcome::Error(msg.clone()),
				})),
				AnalysisReport::PrModuleContributors {
					outcome: AnalysisOutcome::Pass(msg),
					..
				} => Ok(Rc::new(ScoreResult {
					count: db.pr_module_contributors_weight(),
					score: 0,
					outcome: AnalysisOutcome::Pass(msg.to_string()),
				})),
				AnalysisReport::PrModuleContributors {
					outcome: AnalysisOutcome::Fail(msg),
					..
				} => Ok(Rc::new(ScoreResult {
					count: db.pr_module_contributors_weight(),
					score: 1,
					outcome: AnalysisOutcome::Fail(msg.to_string()),
				})),
				_ => Err(hc_error!("phase name does not match analysis")),
			}
		}

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
	score_result: Rc<ScoreResult>,
	mut score_tree: ScoreTree,
	phase: &str,
	parent_node: NodeIndex<u32>,
) -> Result<ScoreTree> {
	let weight = score_result.count as f64;
	let score_increment = score_result.score as i64;

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
		update_phase(phase, ACTIVITY_PHASE)?;
		// Check if this analysis was skipped or generated an error, then format the results accordingly for reporting.
		if db.activity_active() {
			let raw_activity = db.activity_analysis();
			let activity_res = match &raw_activity.as_ref().outcome {
				HCAnalysisOutcome::Error(err) => Err(hc_error!("{:?}", err)),
				HCAnalysisOutcome::Completed(HCAnalysisValue::Basic(av)) => {
					let raw_threshold: u64 = db.activity_week_count_threshold();
					let threshold = HCBasicValue::from(raw_threshold);
					let predicate = ThresholdPredicate::new(
						av.clone(),
						threshold,
						Some("weeks inactivity".to_owned()),
						Ordering::Less,
					);
					Ok(Rc::new(predicate))
				}
				HCAnalysisOutcome::Completed(HCAnalysisValue::Composite(_)) => Err(hc_error!(
					"activity analysis should return a basic u64 type, not {:?}"
				)),
			};
			// Scoring based off of predicate
			let score_result = Rc::new(match activity_res.as_ref() {
				Err(e) => ScoreResult {
					count: db.activity_weight(),
					score: 1,
					outcome: AnalysisOutcome::Error(e.clone()),
				},
				Ok(pred) => {
					// Derive score from predicate, true --> 0, false --> 1
					let passed = pred.pass()?;
					let msg = pred.to_string();
					let outcome = if passed {
						AnalysisOutcome::Pass(msg)
					} else {
						AnalysisOutcome::Fail(msg)
					};
					ScoreResult {
						count: db.activity_weight(),
						score: (!passed) as u64,
						outcome,
					}
				}
			});
			results.activity = Some((activity_res, raw_activity.concerns.clone()));
			score.activity = score_result.outcome.clone();
			match add_node_and_edge_with_score(
				score_result,
				score_tree.clone(),
				ACTIVITY_PHASE,
				practices_node,
			) {
				Ok(score_tree_inc) => {
					score_tree = score_tree_inc;
				}
				_ => return Err(hc_error!("failed to complete {} scoring.", ACTIVITY_PHASE)),
			}
		}

		/*===REVIEW PHASE===*/
		update_phase(phase, REVIEW_PHASE)?;
		let review_analysis = db.review_analysis()?;
		match review_analysis.as_ref() {
			AnalysisReport::None {
				outcome: AnalysisOutcome::Skipped,
			} => results.review = None,
			AnalysisReport::None {
				outcome: AnalysisOutcome::Error(err),
			} => results.review = Some(Err(err.clone())),
			_ => results.review = Some(Ok(review_analysis)),
		}
		let score_result = db.phase_outcome(Rc::new(REVIEW_PHASE.to_string())).unwrap();
		score.review = score_result.outcome.clone();
		match add_node_and_edge_with_score(score_result, score_tree, REVIEW_PHASE, practices_node) {
			Ok(score_tree_inc) => {
				score_tree = score_tree_inc;
			}
			_ => return Err(hc_error!("failed to complete {} scoring.", REVIEW_PHASE)),
		}

		/*===BINARY PHASE===*/
		update_phase(phase, BINARY_PHASE)?;
		let binary_analysis = db.binary_analysis()?;
		match binary_analysis.as_ref() {
			AnalysisReport::None {
				outcome: AnalysisOutcome::Skipped,
			} => results.binary = None,
			AnalysisReport::None {
				outcome: AnalysisOutcome::Error(err),
			} => results.binary = Some(Err(err.clone())),
			_ => results.binary = Some(Ok(binary_analysis)),
		}
		let score_result = db.phase_outcome(Rc::new(BINARY_PHASE.to_string())).unwrap();
		score.binary = score_result.outcome.clone();
		match add_node_and_edge_with_score(score_result, score_tree, BINARY_PHASE, practices_node) {
			Ok(score_tree_inc) => {
				score_tree = score_tree_inc;
			}
			_ => return Err(hc_error!("failed to complete {} scoring.", BINARY_PHASE)),
		}

		/*===IDENTITY PHASE===*/
		update_phase(phase, IDENTITY_PHASE)?;
		let identity_analysis = db.identity_analysis()?;
		match identity_analysis.as_ref() {
			AnalysisReport::None {
				outcome: AnalysisOutcome::Skipped,
			} => results.identity = None,
			AnalysisReport::None {
				outcome: AnalysisOutcome::Error(err),
			} => results.identity = Some(Err(err.clone())),
			_ => results.identity = Some(Ok(identity_analysis)),
		}

		let score_result = db
			.phase_outcome(Rc::new(IDENTITY_PHASE.to_string()))
			.unwrap();
		score.identity = score_result.outcome.clone();
		match add_node_and_edge_with_score(score_result, score_tree, IDENTITY_PHASE, practices_node)
		{
			Ok(score_tree_inc) => {
				score_tree = score_tree_inc;
			}
			_ => return Err(hc_error!("failed to complete {} scoring.", IDENTITY_PHASE)),
		}

		/*===FUZZ PHASE===*/
		update_phase(phase, FUZZ_PHASE)?;
		let fuzz_analysis = db.fuzz_analysis()?;
		match fuzz_analysis.as_ref() {
			AnalysisReport::None {
				outcome: AnalysisOutcome::Skipped,
			} => results.fuzz = None,
			AnalysisReport::None {
				outcome: AnalysisOutcome::Error(err),
			} => results.fuzz = Some(Err(err.clone())),
			_ => results.fuzz = Some(Ok(fuzz_analysis)),
		}

		let score_result = db.phase_outcome(Rc::new(FUZZ_PHASE.to_string())).unwrap();
		score.fuzz = score_result.outcome.clone();
		match add_node_and_edge_with_score(score_result, score_tree, FUZZ_PHASE, practices_node) {
			Ok(score_tree_inc) => {
				score_tree = score_tree_inc;
			}
			_ => return Err(hc_error!("failed to complete {} scoring.", FUZZ_PHASE)),
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
		update_phase(phase, TYPO_PHASE)?;
		let typo_analysis = db.typo_analysis()?;
		match typo_analysis.as_ref() {
			AnalysisReport::None {
				outcome: AnalysisOutcome::Skipped,
			} => results.typo = None,
			AnalysisReport::None {
				outcome: AnalysisOutcome::Error(err),
			} => results.typo = Some(Err(err.clone())),
			_ => results.typo = Some(Ok(typo_analysis)),
		}
		let score_result = db.phase_outcome(Rc::new(TYPO_PHASE.to_string())).unwrap();
		score.typo = score_result.outcome.clone();
		match add_node_and_edge_with_score(score_result, score_tree, TYPO_PHASE, attacks_node) {
			Ok(score_tree_inc) => {
				score_tree = score_tree_inc;
			}
			_ => return Err(hc_error!("failed to complete {} scoring.", TYPO_PHASE)),
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
			update_phase(phase, AFFILIATION_PHASE)?;
			let affiliation_analysis = db.affiliation_analysis()?;
			match affiliation_analysis.as_ref() {
				AnalysisReport::None {
					outcome: AnalysisOutcome::Skipped,
				} => results.affiliation = None,
				AnalysisReport::None {
					outcome: AnalysisOutcome::Error(err),
				} => results.affiliation = Some(Err(err.clone())),
				_ => results.affiliation = Some(Ok(affiliation_analysis)),
			}
			let score_result = db
				.phase_outcome(Rc::new(AFFILIATION_PHASE.to_string()))
				.unwrap();
			score.affiliation = score_result.outcome.clone();
			match add_node_and_edge_with_score(
				score_result,
				score_tree,
				AFFILIATION_PHASE,
				commit_node,
			) {
				Ok(score_tree_inc) => {
					score_tree = score_tree_inc;
				}
				_ => {
					return Err(hc_error!(
						"failed to complete {} scoring.",
						AFFILIATION_PHASE
					))
				}
			}

			/*===NEW_PHASE===*/
			update_phase(phase, CHURN_PHASE)?;
			let churn_analysis = db.churn_analysis()?;
			match churn_analysis.as_ref() {
				AnalysisReport::None {
					outcome: AnalysisOutcome::Skipped,
				} => results.churn = None,
				AnalysisReport::None {
					outcome: AnalysisOutcome::Error(err),
				} => results.churn = Some(Err(err.clone())),
				_ => results.churn = Some(Ok(churn_analysis)),
			}
			let score_result = db.phase_outcome(Rc::new(CHURN_PHASE.to_string())).unwrap();
			score.churn = score_result.outcome.clone();
			match add_node_and_edge_with_score(score_result, score_tree, CHURN_PHASE, commit_node) {
				Ok(score_tree_inc) => {
					score_tree = score_tree_inc;
				}
				_ => return Err(hc_error!("failed to complete {} scoring.", CHURN_PHASE)),
			}

			/*===NEW_PHASE===*/
			update_phase(phase, ENTROPY_PHASE)?;
			let entropy_analysis = db.entropy_analysis()?;
			match entropy_analysis.as_ref() {
				AnalysisReport::None {
					outcome: AnalysisOutcome::Skipped,
				} => results.entropy = None,
				AnalysisReport::None {
					outcome: AnalysisOutcome::Error(err),
				} => results.entropy = Some(Err(err.clone())),
				_ => results.entropy = Some(Ok(entropy_analysis)),
			}

			let score_result = db
				.phase_outcome(Rc::new(ENTROPY_PHASE.to_string()))
				.unwrap();
			score.entropy = score_result.outcome.clone();
			match add_node_and_edge_with_score(score_result, score_tree, ENTROPY_PHASE, commit_node)
			{
				Ok(score_tree_inc) => {
					score_tree = score_tree_inc;
				}
				_ => return Err(hc_error!("failed to complete {} scoring.", ENTROPY_PHASE)),
			}
		}
	}

	let start_node: NodeIndex<u32> = n(0);
	score.total = score_nodes(start_node, score_tree.tree);

	Ok(ScoringResults { results, score })
}

pub fn score_pr_results(phase: &mut Phase, db: &dyn ScoringProvider) -> Result<ScoringResults> {
	/*Scoring should be performed by the construction of a "score tree" where scores are the
	nodes and weights are the edges. The leaves are the analyses themselves, which either
	pass (a score of 0) or fail (a score of 1). These are then combined with the other
	children of their parent according to their weights, repeating until the final score is
	reached.
	generate the tree
	traverse and score recursively
	*/

	let mut results = AnalysisResults::default();
	let mut score = Score::default();
	let mut score_tree = ScoreTree { tree: Graph::new() };
	let root_node = score_tree.tree.add_node(ScoreTreeNode {
		label: RISK_PHASE.to_string(),
		score: -1.0,
		weight: 0.0,
	});

	/* PRACTICES NODE ADDITION */
	// Currently there are no practices analyses for a single pull request analysis

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

			/*===PR_AFFILIATION_PHASE===*/
			update_phase(phase, PR_AFFILIATION_PHASE)?;
			let pr_affiliation_analysis = db.pr_affiliation_analysis()?;
			match pr_affiliation_analysis.as_ref() {
				AnalysisReport::None {
					outcome: AnalysisOutcome::Skipped,
				} => results.affiliation = None,
				AnalysisReport::None {
					outcome: AnalysisOutcome::Error(err),
				} => results.pr_affiliation = Some(Err(err.clone())),
				_ => results.pr_affiliation = Some(Ok(pr_affiliation_analysis)),
			}
			let score_result = db
				.phase_outcome(Rc::new(PR_AFFILIATION_PHASE.to_string()))
				.unwrap();
			score.pr_affiliation = score_result.outcome.clone();
			match add_node_and_edge_with_score(
				score_result,
				score_tree,
				PR_AFFILIATION_PHASE,
				commit_node,
			) {
				Ok(score_tree_inc) => {
					score_tree = score_tree_inc;
				}
				_ => {
					return Err(hc_error!(
						"failed to complete {} scoring.",
						PR_AFFILIATION_PHASE
					))
				}
			}

			/*===PR_CONTRIBUTOR_TRUST_PHASE===*/
			update_phase(phase, PR_CONTRIBUTOR_TRUST_PHASE)?;
			let pr_contributor_trust_analysis = db.pr_contributor_trust_analysis()?;
			match pr_contributor_trust_analysis.as_ref() {
				AnalysisReport::None {
					outcome: AnalysisOutcome::Skipped,
				} => results.pr_contributor_trust = None,
				AnalysisReport::None {
					outcome: AnalysisOutcome::Error(err),
				} => results.pr_contributor_trust = Some(Err(err.clone())),
				_ => results.pr_contributor_trust = Some(Ok(pr_contributor_trust_analysis)),
			}
			let score_result = db
				.phase_outcome(Rc::new(PR_CONTRIBUTOR_TRUST_PHASE.to_string()))
				.unwrap();
			score.pr_contributor_trust = score_result.outcome.clone();
			match add_node_and_edge_with_score(
				score_result,
				score_tree,
				PR_CONTRIBUTOR_TRUST_PHASE,
				commit_node,
			) {
				Ok(score_tree_inc) => {
					score_tree = score_tree_inc;
				}
				_ => {
					return Err(hc_error!(
						"failed to complete {} scoring.",
						PR_CONTRIBUTOR_TRUST_PHASE
					))
				}
			}

			/*===PR_MODULE_CONTRIBUTORS_PHASE===*/
			update_phase(phase, PR_MODULE_CONTRIBUTORS_PHASE)?;
			let pr_module_contributors_analysis = db.pr_module_contributors_analysis()?;
			match pr_module_contributors_analysis.as_ref() {
				AnalysisReport::None {
					outcome: AnalysisOutcome::Skipped,
				} => results.pr_module_contributors = None,
				AnalysisReport::None {
					outcome: AnalysisOutcome::Error(err),
				} => results.pr_module_contributors = Some(Err(err.clone())),
				_ => results.pr_module_contributors = Some(Ok(pr_module_contributors_analysis)),
			}
			let score_result = db
				.phase_outcome(Rc::new(PR_MODULE_CONTRIBUTORS_PHASE.to_string()))
				.unwrap();
			score.pr_module_contributors = score_result.outcome.clone();
			match add_node_and_edge_with_score(
				score_result,
				score_tree,
				PR_MODULE_CONTRIBUTORS_PHASE,
				commit_node,
			) {
				Ok(score_tree_inc) => {
					score_tree = score_tree_inc;
				}
				_ => {
					return Err(hc_error!(
						"failed to complete {} scoring.",
						PR_MODULE_CONTRIBUTORS_PHASE
					))
				}
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
