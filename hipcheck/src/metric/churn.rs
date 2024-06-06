// SPDX-License-Identifier: Apache-2.0

use crate::context::Context as _;
use crate::data::git::Commit;
use crate::error::Result;
use crate::hc_error;
use crate::metric::math::mean;
use crate::metric::math::std_dev;
use crate::metric::MetricProvider;
use crate::TryAny;
use crate::TryFilter;
use crate::F64;
use serde::Serialize;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Debug, Eq, PartialEq, Serialize)]
pub struct ChurnOutput {
	pub commit_churn_freqs: Vec<CommitChurnFreq>,
}

#[derive(Debug, Eq, PartialEq, Serialize)]
pub struct CommitChurnFreq {
	pub commit: Rc<Commit>,
	pub churn: F64,
}

pub fn churn_metric(db: &dyn MetricProvider) -> Result<Rc<ChurnOutput>> {
	log::debug!("running churn metric");

	let commit_diffs = db.commit_diffs().context("failed to get commit diffs")?;

	let commit_diffs = commit_diffs
		.iter()
		.try_filter(|cd| {
			cd.diff
				.file_diffs
				.iter()
				.try_any(|fd| db.is_likely_source_file(Rc::clone(&fd.file_name)))
		})
		.collect::<crate::error::Result<Vec<_>>>()?;

	let mut commit_churns = Vec::new();
	let mut total_files_changed: i64 = 0;
	let mut total_lines_changed: i64 = 0;

	for commit_diff in commit_diffs {
		let source_files = commit_diff
			.diff
			.file_diffs
			.iter()
			.try_filter(|file_diff| db.is_likely_source_file(Rc::clone(&file_diff.file_name)))
			.collect::<crate::error::Result<Vec<_>>>()?;

		// Update files changed.
		let files_changed = source_files.len() as i64;
		total_files_changed += files_changed;

		// Update lines changed.
		let mut lines_changed: i64 = 0;
		for file_diff in &source_files {
			lines_changed += file_diff
				.additions
				.ok_or_else(|| hc_error!("GitHub commits can't be used for churn"))?;
			lines_changed += file_diff
				.deletions
				.ok_or_else(|| hc_error!("GitHub commits can't be used for churn"))?;
		}
		total_lines_changed += lines_changed;

		commit_churns.push(CommitChurn {
			commit: Rc::clone(&commit_diff.commit),
			files_changed,
			lines_changed,
		});
	}

	let mut commit_churn_freqs: Vec<_> = {
		let file_frequencies: HashMap<&str, f64> = commit_churns
			.iter()
			.map(|commit_churn| {
				// avoid dividing by zero.
				if total_files_changed == 0 {
					(commit_churn.commit.hash.as_ref(), 0.0)
				} else {
					(
						commit_churn.commit.hash.as_ref(),
						commit_churn.files_changed as f64 / total_files_changed as f64,
					)
				}
			})
			.collect();

		let line_frequencies: HashMap<&str, f64> = commit_churns
			.iter()
			.map(|commit_churn| {
				// avoid dividing by zero.
				if total_lines_changed == 0 {
					(commit_churn.commit.hash.as_ref(), 0.0)
				} else {
					(
						commit_churn.commit.hash.as_ref(),
						commit_churn.lines_changed as f64 / total_lines_changed as f64,
					)
				}
			})
			.collect();

		commit_churns
			.iter()
			.map(|commit_churn| {
				let hash: &str = commit_churn.commit.hash.as_ref();
				let file_frequency = file_frequencies[hash];
				let line_frequency = line_frequencies[hash];
				// PANIC: Safe to unwrap, beacuse we are creating a valid floating point number
				let churn =
					F64::new(file_frequency * line_frequency * line_frequency * 1_000_000.0)
						.unwrap();

				CommitChurnFreq {
					commit: Rc::clone(&commit_churn.commit),
					churn,
				}
			})
			.collect()
	};

	let churns: Vec<_> = commit_churn_freqs
		.iter()
		.map(|c| c.churn.into_inner())
		.collect();

	let mean =
		mean(&churns).ok_or_else(|| crate::error::Error::msg("failed to get mean churn value"))?;
	let std_dev = std_dev(mean, &churns)
		.ok_or_else(|| crate::error::Error::msg("failed to get churn standard deviation"))?;

	log::trace!("mean of churn scores [mean='{}']", mean);
	log::trace!("standard deviation of churn scores [stddev='{}']", std_dev);

	if std_dev == 0.0 {
		return Err(hc_error!("not enough commits to calculate churn"));
	}

	for commit_churn_freq in &mut commit_churn_freqs {
		commit_churn_freq.churn = (commit_churn_freq.churn - mean) / std_dev;
	}

	log::info!("completed churn metric");

	Ok(Rc::new(ChurnOutput { commit_churn_freqs }))
}

#[derive(Debug)]
pub struct CommitChurn {
	commit: Rc<Commit>,
	files_changed: i64,
	lines_changed: i64,
}
