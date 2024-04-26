// SPDX-License-Identifier: Apache-2.0

//! Query group for dependencies information.

use crate::npm::{get_package_file, PackageFile};
use crate::Dependencies;
use hc_common::{salsa, Result};
use hc_source::SourceQuery;
use hc_version::VersionQuery;
use std::rc::Rc;

/// Queries about dependencies
#[salsa::query_group(DependenciesProviderStorage)]
pub trait DependenciesProvider: SourceQuery + VersionQuery {
	/// Returns the `Dependencies` struct for the current session
	fn dependencies(&self) -> Result<Rc<Dependencies>>;

	/// The parsed contents of the `package.json` file.
	fn package_file(&self) -> Result<Rc<PackageFile>>;

	/// The value of the `main` field in `package.json`.
	fn package_file_main(&self) -> Result<Rc<String>>;
}

/// Derived query implementations.  Return value is wrapped in an `Rc`
/// to keep cloning cheap.

fn dependencies(db: &dyn DependenciesProvider) -> Result<Rc<Dependencies>> {
	Dependencies::resolve(&db.local(), (&db.npm_version().as_ref()).to_string()).map(Rc::new)
}

fn package_file(db: &dyn DependenciesProvider) -> Result<Rc<PackageFile>> {
	get_package_file(&db.local()).map(Rc::new)
}

fn package_file_main(db: &dyn DependenciesProvider) -> Result<Rc<String>> {
	db.package_file()
		.map(|package_file| package_file.main.clone())
		.map(Rc::new)
}
