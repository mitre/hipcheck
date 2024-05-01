// SPDX-License-Identifier: Apache-2.0

use crate::MetricProvider;
use hc_common::context::Context as _;
use hc_common::{
	error::Result,
	log,
	serde::{self, Serialize},
};
use hc_data::{git::Contributor, Module};
use std::{collections::HashMap, rc::Rc};

#[derive(Debug, Eq, PartialEq, Serialize)]
#[serde(crate = "self::serde")]
pub struct ModuleContributorsOutput {
	pub contributors_map: Rc<HashMap<Rc<Contributor>, Vec<ContributedModule>>>,
}

#[derive(Debug, Eq, PartialEq, Serialize)]
#[serde(crate = "self::serde")]
pub struct ContributedModule {
	pub module: Rc<Module>,
	pub new_contributor: bool,
}

pub fn pr_module_contributors_metric(
	db: &dyn MetricProvider,
) -> Result<Rc<ModuleContributorsOutput>> {
	log::debug!("running pull request module contributors metric");

	let pull_request = db
		.single_pull_request_review()
		.context("failed to get pull request")?;

	let mut contributors_map = HashMap::new();

	let commits = &pull_request.commits;
	// Initialize hash map of contributors
	let contributors = &pull_request.contributors;
	for contributor in contributors {
		contributors_map.insert(contributor, HashMap::new());
	}

	for commit in commits.iter() {
		log::debug!("commit: {:?}", commit);
		let author = db.get_pr_contributors_for_commit(commit.clone())?.author;
		let committer = db.get_pr_contributors_for_commit(commit.clone())?.committer;
		let modules = db
			.modules_for_commit(commit.clone())
			.context("Could not get modules.")?;
		log::debug!("modules: {:?}", modules);

		for module in modules.iter() {
			if !(contributors_map.get(&author).unwrap().contains_key(&module)) {
				contributors_map
					.get(&author)
					.unwrap()
					.to_owned()
					.insert(module, true);
			}

			if !(contributors_map
				.get(&committer)
				.unwrap()
				.contains_key(&module))
			{
				contributors_map
					.get(&committer)
					.unwrap()
					.to_owned()
					.insert(module, true);
			}

			let module_commits = db.commits_for_module(module.to_owned())?;
			for module_commit in module_commits.iter() {
				let module_author = db.contributors_for_commit(module_commit.clone())?.author;
				let module_committer = db.contributors_for_commit(module_commit.clone())?.committer;

				if author == module_author || author == module_committer {
					contributors_map
						.get(&author)
						.unwrap()
						.to_owned()
						.remove(&module);
					contributors_map
						.get(&author)
						.unwrap()
						.to_owned()
						.insert(module, false);
				}

				if committer == module_author || committer == module_committer {
					contributors_map
						.get(&committer)
						.unwrap()
						.to_owned()
						.remove(&module);
					contributors_map
						.get(&committer)
						.unwrap()
						.to_owned()
						.insert(module, false);
				}
			}
		}
	}

	let mut final_contributors_map = HashMap::new();

	for (key, module_map) in contributors_map.iter() {
		let mut module_vector = Vec::new();
		for (inner_key, new_contributor) in module_map.iter() {
			module_vector.push(ContributedModule {
				module: inner_key.to_owned().to_owned(),
				new_contributor: new_contributor.to_owned(),
			})
		}
		final_contributors_map.insert(key.to_owned().to_owned(), module_vector);
	}

	Ok(Rc::new(ModuleContributorsOutput {
		contributors_map: Rc::new(final_contributors_map),
	}))
}
