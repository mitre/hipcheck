// SPDX-License-Identifier: Apache-2.0

use crate::metric::binary_detector::detect_binary_files;
use crate::MetricProvider;
use hc_common::{
	error::Result,
	log,
	serde::{self, Serialize},
	TryFilter,
};
use std::rc::Rc;

#[derive(Debug, Eq, PartialEq, Serialize)]
#[serde(crate = "self::serde")]
pub struct BinaryOutput {
	pub binary_files: Vec<Rc<String>>,
}

/// Determine which files in a repository are of a binary format.
///
/// Collect the paths to all non-plaintext files, filter out non-code
/// binaries (like images or audio, which may be valid parts of a project's
/// source), and return the rest to be counted for Hipcheck's report.
pub fn binary_metric(db: &dyn MetricProvider) -> Result<Rc<BinaryOutput>> {
	log::debug!("running binary metric");

	let pathbuf_rc = db.local();
	let binary_files = detect_binary_files(&pathbuf_rc)?
		.into_iter()
		.try_filter(|f| db.is_likely_binary_file(Rc::clone(f)))
		.collect::<hc_common::error::Result<_>>()?;

	log::info!("completed binary metric");

	Ok(Rc::new(BinaryOutput { binary_files }))
}
