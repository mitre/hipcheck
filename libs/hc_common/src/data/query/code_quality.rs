// SPDX-License-Identifier: Apache-2.0

//! Query group for code quality (linting) information.

use std::rc::Rc;

use crate::data::code_quality::{get_eslint_report, CodeQualityReport};
use crate::data::source::SourceQuery;
use crate::{error::Result, salsa, version::VersionQuery};

/// Queries about code quality
#[salsa::query_group(CodeQualityProviderStorage)]
pub trait CodeQualityProvider: SourceQuery + VersionQuery {
	/// Returns ESLint's report on the source code.
	fn eslint_report(&self) -> Result<Rc<CodeQualityReport>>;
}

/// Derived query implementation.  Return value is wrapped in an `Rc`
/// to keep cloning cheap.

fn eslint_report(db: &dyn CodeQualityProvider) -> Result<Rc<CodeQualityReport>> {
	get_eslint_report(&db.local(), (&db.eslint_version().as_ref()).to_string()).map(Rc::new)
}
