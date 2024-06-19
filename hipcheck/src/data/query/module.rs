// SPDX-License-Identifier: Apache-2.0

//! Query group for module information.

use crate::data::associate_modules_and_commits;
use crate::data::git::Commit;
use crate::data::git::GitProvider;
use crate::data::Module;
use crate::data::ModuleGraph;
use crate::error::Error;
use crate::error::Result;
use pathbuf::pathbuf;
use std::path::PathBuf;
use std::sync::Arc;

/// A module and an associated commit
pub type ModuleCommitMap = Arc<Vec<(Arc<Module>, Arc<Commit>)>>;

/// Queries about modules
#[salsa::query_group(ModuleProviderStorage)]
pub trait ModuleProvider: GitProvider {
	/// Returns output of module analysis on the source code.
	#[salsa::dependencies]
	fn get_module_graph(&self) -> Result<Arc<ModuleGraph>>;

	/// Returns an association list of modules and commits
	fn commits_for_modules(&self) -> Result<ModuleCommitMap>;

	/// Returns the commits associated with a particular module
	fn commits_for_module(&self, module: Arc<Module>) -> Result<Arc<Vec<Arc<Commit>>>>;

	/// Returns the modules associated with a particular commit
	fn modules_for_commit(&self, commit: Arc<Commit>) -> Result<Arc<Vec<Arc<Module>>>>;

	/// Returns the directory containing the data files
	#[salsa::input]
	fn data_dir(&self) -> Arc<PathBuf>;

	/// Returns the location of the module-deps.js file
	fn module_deps(&self) -> Result<Arc<PathBuf>>;
}

/// Derived query implementations.  Return values are wrapped in an
/// `Rc` to keep cloning cheap.

fn get_module_graph(db: &dyn ModuleProvider) -> Result<Arc<ModuleGraph>> {
	let module_deps = db.module_deps()?;
	ModuleGraph::get_module_graph_from_repo(&db.local(), module_deps.as_ref()).map(Arc::new)
}

fn commits_for_modules(db: &dyn ModuleProvider) -> Result<ModuleCommitMap> {
	let repo_path = db.local();
	let commits = db.commits()?;
	let modules = db.get_module_graph()?;
	associate_modules_and_commits(repo_path.as_ref(), modules, commits)
}

fn commits_for_module(
	db: &dyn ModuleProvider,
	module: impl AsRef<Module>,
) -> Result<Arc<Vec<Arc<Commit>>>> {
	let commits = db
		.commits_for_modules()?
		.iter()
		.filter_map(|(m, c)| (**m == *module.as_ref()).then(|| Arc::clone(c)))
		.collect();

	Ok(Arc::new(commits))
}

fn modules_for_commit(
	db: &dyn ModuleProvider,
	commit: impl AsRef<Commit>,
) -> Result<Arc<Vec<Arc<Module>>>> {
	let modules = db
		.commits_for_modules()?
		.iter()
		.filter_map(|(m, c)| (**c == *commit.as_ref()).then(|| Arc::clone(m)))
		.collect();

	Ok(Arc::new(modules))
}

fn module_deps(db: &dyn ModuleProvider) -> Result<Arc<PathBuf>> {
	let data_path = db.data_dir();
	let module_deps_path = pathbuf![data_path.as_ref(), "module-deps.js"];
	if module_deps_path.exists() {
		Ok(Arc::new(module_deps_path))
	} else {
		Err(Error::msg(
			"module-deps.js missing from Hipcheck data folder",
		))
	}
}
