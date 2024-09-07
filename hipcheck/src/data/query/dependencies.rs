// SPDX-License-Identifier: Apache-2.0

//! Query group for dependencies information.

use crate::{
	data::{
		npm::{get_package_file, PackageFile},
		Dependencies,
	},
	error::Result,
	source::SourceQuery,
	version::VersionQuery,
};
use std::sync::Arc;

/// Queries about dependencies
#[salsa::query_group(DependenciesProviderStorage)]
pub trait DependenciesProvider: SourceQuery + VersionQuery {
	/// Returns the `Dependencies` struct for the current session
	fn dependencies(&self) -> Result<Arc<Dependencies>>;

	/// The parsed contents of the `package.json` file.
	fn package_file(&self) -> Result<Arc<PackageFile>>;

	/// The value of the `main` field in `package.json`.
	fn package_file_main(&self) -> Result<Arc<String>>;
}

/// Derived query implementations.  Return value is wrapped in an `Rc`
/// to keep cloning cheap.

fn dependencies(db: &dyn DependenciesProvider) -> Result<Arc<Dependencies>> {
	Dependencies::resolve(&db.local(), (&db.npm_version().as_ref()).to_string()).map(Arc::new)
}

fn package_file(db: &dyn DependenciesProvider) -> Result<Arc<PackageFile>> {
	get_package_file(&db.local()).map(Arc::new)
}

fn package_file_main(db: &dyn DependenciesProvider) -> Result<Arc<String>> {
	db.package_file()
		.map(|package_file| package_file.main.clone())
		.map(Arc::new)
}
