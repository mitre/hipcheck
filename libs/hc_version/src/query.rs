// SPDX-License-Identifier: Apache-2.0

//! A query group for current versions of Hipcheck and tool
//! dependencies.

use hc_common::salsa;
use std::rc::Rc;

/// Queries for current versions of Hipcheck and tool dependencies
#[salsa::query_group(VersionQueryStorage)]
pub trait VersionQuery: salsa::Database {
	/// Returns the current Hipcheck version
	#[salsa::input]
	fn hc_version(&self) -> Rc<String>;
	/// Returns the version of npm currently running on user's machine
	#[salsa::input]
	fn npm_version(&self) -> Rc<String>;
	/// Returns the version of eslint currently running on user's machine
	#[salsa::input]
	fn eslint_version(&self) -> Rc<String>;
	/// Returns the version of git currently running on user's machine
	#[salsa::input]
	fn git_version(&self) -> Rc<String>;
}
