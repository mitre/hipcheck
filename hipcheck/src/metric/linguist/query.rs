// SPDX-License-Identifier: Apache-2.0

//! A query group for source file language detection queries.

use crate::{
	config::LanguagesConfigQuery,
	error::{Context, Result},
	metric::linguist::SourceFileDetector,
};
use std::sync::Arc;

/// Queries related to source file language detection
#[salsa::query_group(LinguistStorage)]
pub trait Linguist: LanguagesConfigQuery {
	/// Returns the code detector for the current session
	fn source_file_detector(&self) -> Result<Arc<SourceFileDetector>>;

	/// Returns likely source file assessment for `file_name`
	fn is_likely_source_file(&self, file_name: Arc<String>) -> Result<bool>;
}

/// Derived query implementations

fn source_file_detector(db: &dyn Linguist) -> Result<Arc<SourceFileDetector>> {
	let detector = SourceFileDetector::load(db.langs_file()?.as_ref())
		.context("failed to build a source file detector from langs file")?;

	Ok(Arc::new(detector))
}

fn is_likely_source_file(db: &dyn Linguist, file_name: Arc<String>) -> Result<bool> {
	let detector = db
		.source_file_detector()
		.context("failed to get source file detector")?;

	Ok(detector.is_likely_source_file(file_name.as_ref()))
}
