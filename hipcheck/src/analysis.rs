// SPDX-License-Identifier: Apache-2.0

//! Policy-derived analysis tree types and helpers.

use crate::{
	F64,
	engine::HcEngine,
	error::Result,
	hc_error,
	policy::{
		PolicyFile,
		policy_file::{PolicyAnalysis, PolicyCategory, PolicyCategoryChild},
	},
	policy_exprs::Expr,
	score::{PluginAnalysisResult, ScoreTreeNode},
	session::Session,
};
use indextree::{Arena, NodeEdge, NodeId};
use num_traits::identities::Zero;
use std::{collections::HashMap, rc::Rc};

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
		analysis: Box<PoliciedAnalysis>,
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
			analysis: Box::new(PoliciedAnalysis(analysis, opt_policy)),
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
		let res = visit_leaves(
			self.root,
			&self.tree,
			|_n| false,
			|_a, n| match n {
				AnalysisTreeNode::Analysis { analysis, .. } => analysis.clone(),
				AnalysisTreeNode::Category { .. } => unreachable!(),
			},
		);

		res.into_iter().map(|boxed| *boxed).collect()
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
			NodeEdge::Start(n) => {
				last_start = n;
				scope.push(acc_op(tree.get(n).unwrap().get()));
			}
			NodeEdge::End(n) => {
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

pub fn mutate_leaves<T, F>(node: NodeId, tree: &mut Arena<T>, op: F) -> Result<()>
where
	F: Fn(&mut T) -> Result<()>,
{
	let mut last_start: NodeId = node;
	let edges: Vec<_> = node.traverse(tree).collect();
	for edge in edges {
		match edge {
			NodeEdge::Start(n) => {
				last_start = n;
			}
			NodeEdge::End(n) => {
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

	let update_policy = |node: &mut AnalysisTreeNode| -> Result<()> {
		if let AnalysisTreeNode::Analysis { analysis, .. } = node {
			let a: &Analysis = &analysis.0;
			if analysis.1.is_none() {
				analysis.1 = Some(
					session
						.default_policy_expr(a.publisher.clone(), a.plugin.clone())?
						.ok_or(hc_error!(
							"plugin {}::{} does not have a default policy, please define a policy in your policy file",
							a.publisher.clone(),
							a.plugin.clone()
						))?,
				);
			}
		}
		Ok(())
	};

	mutate_leaves(res_tree.root, &mut res_tree.tree, update_policy)?;

	Ok(Rc::new(res_tree))
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
