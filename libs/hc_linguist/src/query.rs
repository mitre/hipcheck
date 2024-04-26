// SPDX-License-Identifier: Apache-2.0

//! A query group for source file language detection queries.

use crate::SourceFileDetector;
use hc_common::{
	context::Context,
	error::Result,
	salsa,
};
use hc_config::LanguagesConfigQuery;
use std::rc::Rc;

/// Queries related to source file language detection
#[salsa::query_group(LinguistStorage)]
pub trait Linguist: LanguagesConfigQuery {
	/// Returns the code detector for the current session
	fn source_file_detector(&self) -> Result<Rc<SourceFileDetector>>;

	/// Returns likely source file assessment for `file_name`
	fn is_likely_source_file(&self, file_name: Rc<String>) -> Result<bool>;
}

/// Derived query implementations

fn source_file_detector(db: &dyn Linguist) -> Result<Rc<SourceFileDetector>> {
	let detector = SourceFileDetector::load(db.langs_file().as_ref())
		.context("failed to build a source file detector from langs file")?;

	Ok(Rc::new(detector))
}

fn is_likely_source_file(db: &dyn Linguist, file_name: Rc<String>) -> Result<bool> {
	let detector = db
		.source_file_detector()
		.context("failed to get source file detector")?;

	Ok(detector.is_likely_source_file(file_name.as_ref()))
}
