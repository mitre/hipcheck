// SPDX-License-Identifier: Apache-2.0

use crate::{
	error::Result,
	metric::{binary_detector::detect_binary_files, MetricProvider},
	TryFilter,
};
use serde::Serialize;
use std::sync::Arc;

#[derive(Debug, Eq, PartialEq, Serialize)]
pub struct BinaryOutput {
	pub binary_files: Vec<Arc<String>>,
}

/// Determine which files in a repository are of a binary format.
///
/// Collect the paths to all non-plaintext files, filter out non-code
/// binaries (like images or audio, which may be valid parts of a project's
/// source), and return the rest to be counted for Hipcheck's report.
pub fn binary_metric(db: &dyn MetricProvider) -> Result<Arc<BinaryOutput>> {
	log::debug!("running binary metric");

	let pathbuf_rc = db.local();
	let binary_files = detect_binary_files(&pathbuf_rc)?
		.into_iter()
		.try_filter(|f| db.is_likely_binary_file(Arc::clone(f)))
		.collect::<crate::error::Result<_>>()?;

	log::info!("completed binary metric");

	Ok(Arc::new(BinaryOutput { binary_files }))
}
