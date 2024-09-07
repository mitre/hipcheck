// SPDX-License-Identifier: Apache-2.0

//! A query group for binary file detection queries.

use crate::{
	config::PracticesConfigQuery,
	error::{Context as _, Result},
	metric::binary_detector::BinaryFileDetector,
};
use std::sync::Arc;

/// Queries related to binary file detection
#[salsa::query_group(BinaryFileStorage)]
pub trait BinaryFile: PracticesConfigQuery {
	/// Returns the binary file detector for the current session
	fn binary_file_detector(&self) -> Result<Arc<BinaryFileDetector>>;

	/// Returns likely binary file assessment for `file_name`
	fn is_likely_binary_file(&self, file_name: Arc<String>) -> Result<bool>;
}

/// Derived query implementations

fn binary_file_detector(db: &dyn BinaryFile) -> Result<Arc<BinaryFileDetector>> {
	let detector = BinaryFileDetector::load(db.binary_formats_file()?.as_ref())
		.context("failed to build a binary file detector from binary format file")?;

	Ok(Arc::new(detector))
}

fn is_likely_binary_file(db: &dyn BinaryFile, file_name: Arc<String>) -> Result<bool> {
	let detector = db
		.binary_file_detector()
		.context("failed to get binary file detector")?;

	Ok(detector.is_likely_binary_file(file_name.as_ref()))
}
