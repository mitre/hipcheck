// SPDX-License-Identifier: Apache-2.0

//! A query group for accessing Git repository data.

use crate::target::{RemoteGitRepo, Target};
use std::path::PathBuf;
use std::sync::Arc;

/// Queries for accessing info about a Git source
#[salsa::query_group(SourceQueryStorage)]
pub trait SourceQuery: salsa::Database {
	/// Returns the input `Target` struct
	#[salsa::input]
	fn target(&self) -> Arc<Target>;

	/// Returns the local path to the repository
	fn local(&self) -> Arc<PathBuf>;
	/// Returns remote source information, if any
	fn remote(&self) -> Option<Arc<RemoteGitRepo>>;
	/// Returns the repository HEAD commit
	fn head(&self) -> Arc<String>;
	/// Returns the repository name
	fn name(&self) -> Arc<String>;
	/// Returns the repository url
	fn url(&self) -> Option<Arc<String>>;
}

/// Derived query implementations

/// These return the value of a particular field in the input `Target`
/// struct.  Since all are owned types, the values are wrapped in an
/// `Rc` to keep cloning cheap.

fn local(db: &dyn SourceQuery) -> Arc<PathBuf> {
	let target = db.target();
	Arc::new(target.local.path.clone())
}

fn remote(db: &dyn SourceQuery) -> Option<Arc<RemoteGitRepo>> {
	db.target()
		.remote
		.as_ref()
		.map(|remote| Arc::new(remote.clone()))
}

fn head(db: &dyn SourceQuery) -> Arc<String> {
	let target = db.target();
	Arc::new(target.local.git_ref.clone())
}

fn name(db: &dyn SourceQuery) -> Arc<String> {
	let target = db.target();
	// In the future may want to augment Target/LocalGitRepo with a
	// "name" field. For now, treat the dir name of the repo as the name
	Arc::new(
		target
			.local
			.path
			.as_path()
			.file_name()
			.unwrap()
			.to_str()
			.unwrap()
			.to_owned(),
	)
}

fn url(db: &dyn SourceQuery) -> Option<Arc<String>> {
	Some(Arc::new(db.remote()?.url.to_string()))
}
