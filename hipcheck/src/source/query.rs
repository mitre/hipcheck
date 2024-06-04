// SPDX-License-Identifier: Apache-2.0

//! A query group for accessing Git repository data.

use crate::hash;
use crate::source::source::Remote;
use crate::source::source::Source;
use pathbuf::pathbuf;
use std::path::PathBuf;
use std::rc::Rc;

/// Queries for accessing info about a Git source
#[salsa::query_group(SourceQueryStorage)]
pub trait SourceQuery: salsa::Database {
	/// Returns the input `Source` struct
	#[salsa::input]
	#[salsa::query_type(SourceTypeQuery)]
	fn source(&self) -> Rc<Source>;

	/// Returns the local path to the repository
	fn local(&self) -> Rc<PathBuf>;
	/// Returns remote source information, if any
	fn remote(&self) -> Option<Rc<Remote>>;
	/// Returns the repository HEAD commit
	fn head(&self) -> Rc<String>;
	/// Returns the repository name
	fn name(&self) -> Rc<String>;
	/// Returns the repository url
	fn url(&self) -> Option<Rc<String>>;
	/// Returns a filesystem-usable storage path for the source
	fn storage_path(&self) -> Rc<PathBuf>;
}

/// Derived query implementations

/// These return the value of a particular field in the input `Source`
/// struct.  Since all are owned types, the values are wrapped in an
/// `Rc` to keep cloning cheap.

// PANIC: It is safe to unwrap in these functions, because a valid
// `Source` will always contain a `SourceRepo`, so `get_repo()` will
// not return an error here.
fn local(db: &dyn SourceQuery) -> Rc<PathBuf> {
	let source = db.source();
	Rc::new(source.get_repo().local().to_path_buf())
}

fn remote(db: &dyn SourceQuery) -> Option<Rc<Remote>> {
	db.source()
		.get_remote()
		.as_ref()
		.map(|remote| Rc::new(remote.clone()))
}

fn head(db: &dyn SourceQuery) -> Rc<String> {
	let source = db.source();
	Rc::new(source.get_repo().head().to_string())
}

fn name(db: &dyn SourceQuery) -> Rc<String> {
	let source = db.source();
	Rc::new(source.get_repo().name().to_string())
}

fn url(db: &dyn SourceQuery) -> Option<Rc<String>> {
	Some(Rc::new(db.remote()?.url().to_string()))
}

// Computes the appropriate path based on the remote or local info
fn storage_path(db: &dyn SourceQuery) -> Rc<PathBuf> {
	use crate::source::source::Remote::*;

	let path_buf = match db.remote() {
		Some(remote) => match remote.as_ref() {
			// This is a GitHub remote repository source.
			GitHub { owner, repo, .. } => pathbuf!["remote", "github", owner, repo],
			// This is a GitHub remote pull request source.
			GitHubPr {
				owner,
				repo,
				pull_request,
				..
			} => pathbuf!["remote", "github", owner, repo, &pull_request.to_string()],
			// This is an unknown remote source.
			Unknown(url) => pathbuf!["remote", "unknown", &hash!(url).to_string()],
		},
		// This is a local source.
		None => match db.local().file_name() {
			Some(file_name) => pathbuf!["local", file_name],
			None => pathbuf!["local", "unknown", &hash!(db.local()).to_string()],
		},
	};

	Rc::new(path_buf)
}
