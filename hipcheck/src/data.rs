// SPDX-License-Identifier: Apache-2.0

//! Functions and types for data retrieval.

pub mod git;
pub mod git_command;
pub mod npm;
pub mod source;

mod code_quality;
mod es_lint;
mod github;
mod hash;
mod modules;
mod query;

pub use query::*;

use crate::context::Context;
use crate::error::Error;
use crate::error::Result;
use crate::hc_error;
use git::get_commits_for_file;
use git::Commit;
use git::CommitContributor;
use git::Contributor;
use git::Diff;
use github::*;
use modules::RawModule;
use pathbuf::pathbuf;
use petgraph::visit::Dfs;
use petgraph::Graph;
use serde::Serialize;
use std::collections::HashSet;
use std::path::Path;
use std::rc::Rc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependencies {
	pub language: Lang,
	pub deps: Vec<Rc<String>>,
}

impl Dependencies {
	pub fn resolve(repo: &Path, version: String) -> Result<Dependencies> {
		match Lang::detect(repo) {
			language @ Lang::JavaScript => {
				let deps = npm::get_dependencies(repo, version)?
					.into_iter()
					.map(Rc::new)
					.collect();
				Ok(Dependencies { language, deps })
			}
			Lang::Unknown => Err(Error::msg(
				"can't identify a known language in the repository",
			)),
		}
	}
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum Lang {
	JavaScript,
	Unknown,
}

impl Lang {
	fn detect(repo: &Path) -> Lang {
		if pathbuf![repo, "package.json"].exists() {
			Lang::JavaScript
		} else {
			Lang::Unknown
		}
	}
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Fuzz {
	pub exists: bool,
}

pub fn get_fuzz_check(token: &str, repo_uri: Rc<String>) -> Result<Fuzz> {
	let github = GitHub::new("google", "oss-fuzz", token)?;

	let github_result = github
		.fuzz_check(repo_uri)
		.context("unable to query fuzzing info")?;

	let result = Fuzz {
		exists: github_result,
	};

	Ok(result)
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct PullRequest {
	pub id: u64,
	pub reviews: u64,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct SinglePullRequest {
	pub id: u64,
	pub reviews: u64,
	pub commits: Vec<Rc<Commit>>,
	pub contributors: Vec<Rc<Contributor>>,
	pub commit_contributors: Vec<CommitContributor>,
	pub diffs: Vec<Rc<Diff>>,
}

pub fn get_pull_request_reviews_from_github(
	owner: &str,
	repo: &str,
	token: &str,
) -> Result<Vec<PullRequest>> {
	let github = GitHub::new(owner, repo, token)?;

	let results = github
		.get_reviews_for_pr()
		.context("failed to get pull request reviews from the GitHub API, please check the HC_GITHUB_TOKEN system environment variable")?
		.into_iter()
		.map(|pr| PullRequest {
			id: pr.number,
			reviews: pr.reviews,
		})
		.collect();

	Ok(results)
}

pub fn get_single_pull_request_review_from_github(
	owner: &str,
	repo: &str,
	pull_request: &u64,
	token: &str,
) -> Result<SinglePullRequest> {
	let github_pr = GitHubPr::new(owner, repo, pull_request, token)?;

	let github_result = github_pr
		.get_review_for_single_pr()
		.context("failed to get pull request review from the GitHub API")?;

	log::trace!("full pull request is {:#?}", github_result);

	let commits = github_result
		.commits
		.iter()
		.map(|raw| {
			Rc::new(Commit {
				hash: raw.hash.to_owned(),
				written_on: raw.written_on.to_owned(),
				committed_on: raw.committed_on.to_owned(),
			})
		})
		.collect();

	let mut contributors: Vec<Rc<Contributor>> = github_result
		.commits
		.iter()
		.flat_map(|raw| {
			[
				Rc::new(raw.author.to_owned()),
				Rc::new(raw.committer.to_owned()),
			]
		})
		.collect();

	contributors.sort();
	contributors.dedup();

	let commit_contributors = github_result
		.commits
		.iter()
		.enumerate()
		.map(|(commit_id, raw)| {
			// SAFETY: These `position` calls are guaranteed to return `Some`
			// given how `contributors` is constructed from `get_review_for_single_pr()`
			let author_id = contributors
				.iter()
				.position(|c| c.as_ref() == &raw.author)
				.unwrap();
			let committer_id = contributors
				.iter()
				.position(|c| c.as_ref() == &raw.author)
				.unwrap();

			CommitContributor {
				commit_id,
				author_id,
				committer_id,
			}
		})
		.collect();

	let diffs = github_result
		.diffs
		.iter()
		.map(|diff| Rc::new(diff.to_owned()))
		.collect();

	let result = SinglePullRequest {
		id: github_result.pull_request.number,
		reviews: github_result.pull_request.reviews,
		commits,
		contributors,
		commit_contributors,
		diffs,
	};

	Ok(result)
}

// Module structs/enums

#[derive(Debug, PartialEq, Eq, Copy, Clone, Serialize)]
pub enum Relationship {
	Child,
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, Serialize)]
pub struct Module {
	pub file: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ModuleGraph {
	pub connections: Graph<Module, Relationship>,
}

// For a given ModuleGraph representation of repository files, associate each module with the file's commit hashes
pub fn associate_modules_and_commits(
	repo_path: &Path,
	module_graph: Rc<ModuleGraph>,
	commits: Rc<Vec<Rc<Commit>>>,
) -> Result<ModuleCommitMap> {
	// Vector containing pairings between module and associated commits
	let mut modules_commits: Vec<(Rc<Module>, Rc<Commit>)> = Vec::new();

	// Graph traversal, depth-first
	let start = module_graph
		.connections
		.node_indices()
		.next()
		.ok_or_else(|| hc_error!("no nodes in the module graph"))?;
	let mut dfs = Dfs::new(&module_graph.connections, start);

	// Loop through adjoining nodes in graph
	while let Some(visited) = dfs.next(&module_graph.connections) {
		let hashes_raw = get_commits_for_file(repo_path, &module_graph.connections[visited].file)?;

		// Get hashes associated with this file
		let hashes = hashes_raw.lines().collect::<HashSet<_>>();

		// Get all commits referencing the hashes for current module in loop
		let commit_vec = commits
			.iter()
			.filter_map(|commit| {
				if hashes.contains(&commit.hash.as_ref()) {
					Some(Rc::clone(commit))
				} else {
					None
				}
			})
			.collect::<Vec<_>>();

		// Add entry to vec for each commit e.g. <Module, Commit>
		for commit in commit_vec {
			modules_commits.push((
				Rc::new(module_graph.connections[visited].clone()),
				Rc::clone(&commit),
			));
		}
	}

	Ok(Rc::new(modules_commits))
}

impl ModuleGraph {
	// Calls node library module-deps, converts json to graph model of modules for analysis
	pub fn get_module_graph_from_repo(repo: &Path, module_deps: &Path) -> Result<ModuleGraph> {
		let raw_modules = modules::generate_module_model(repo, module_deps)
			.context("Unable to generate module model")?;
		module_graph_from_raw(raw_modules)
	}
}

// Implement a crude Eq and PartialEq trait (should revisit this as we evolve the module functionality)
impl Eq for ModuleGraph {}

impl PartialEq for ModuleGraph {
	fn eq(&self, _other: &Self) -> bool {
		false
	}
}

fn module_graph_from_raw(raw_modules: Vec<RawModule>) -> Result<ModuleGraph> {
	let mut graph = Graph::new();
	let mut node_idxs = Vec::new();

	for raw_module in raw_modules {
		let node = Module {
			file: raw_module.file,
		};

		let node_idx = graph.add_node(node);

		// Record the node index.
		node_idxs.push(node_idx);

		for (_, dep) in raw_module.deps {
			// Check if `dep` is a node already. Add if it isn't, get
			// index to it.
			let node = Module { file: dep };

			let mut known_idx = None;

			for idx in &node_idxs {
				if graph[*idx] == node {
					known_idx = Some(*idx);
				}
			}

			let dep_idx = known_idx.unwrap_or_else(|| graph.add_node(node));

			graph.add_edge(node_idx, dep_idx, Relationship::Child);
		}
	}

	Ok(ModuleGraph { connections: graph })
}
