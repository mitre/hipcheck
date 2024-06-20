// SPDX-License-Identifier: Apache-2.0

use crate::context::Context as _;
use crate::data::git::Commit;
use crate::data::git::CommitDiff;
use crate::error::Result;
use crate::hc_error;
use crate::metric::math::mean;
use crate::metric::math::std_dev;
use crate::metric::MetricProvider;
use crate::TryAny;
use crate::TryFilter;
use crate::F64;
use dashmap::DashMap;
use finl_unicode::grapheme_clusters::Graphemes;
use rayon::prelude::*;
use serde::Serialize;
use std::collections::HashMap;
use std::iter::Iterator;
use std::ops::Not as _;
use std::sync::Arc;
use unicode_normalization::UnicodeNormalization;

/// Analyze a source to produce a set of entropy scores for its commits.
///
/// # Algorithm
///
/// The entropy algorithm works roughly as follows:
///
/// 1. Get the list of all commits for a repository.
/// 2. Filter out the commits which do not contain any source-code changes.
/// 3. Calculate the frequencies of graphemes in the source-file patches (additions and deletions)
///    for each commit.
/// 4. Calculate the overall frequencies of each grapheme across all source-file patches from all
///    commits.
/// 5. Calculate the "commit entropy" score for each commit as the sum of each grapheme frequency
///    times the log base 2 of the grapheme frequency divided by the total frequency.
/// 6. Normalize these "commit entropy" scores into Z-scores.
///
/// The idea here is that this metric captures the degree of textual randomness, but does _not_
/// incorporate any positional information. It is solely based on the frequency of graphemes
/// in the patch text.
pub fn entropy_metric(db: &dyn MetricProvider) -> Result<Arc<EntropyOutput>> {
	log::debug!("running entropy metric");

	// Calculate the grapheme frequencies for each commit which contains code.
	let commit_freqs = db
		.commit_diffs()
		.context("failed to get commit diffs")?
		.iter()
		.try_filter(|cd| is_likely_source_file(cd, db))
		.map(|result| match result {
			Ok(commit_diff) => grapheme_freqs(commit_diff, db),
			Err(e) => Err(e),
		})
		.collect::<Result<Vec<_>>>()?;

	// Calculate baseline grapheme frequencies across all commits which contain code.
	let baseline_freqs = baseline_freqs(&commit_freqs);

	// Calculate the entropy of each commit which contains code.
	let mut commit_entropies = commit_freqs
		.iter()
		.map(|commit_freq| commit_entropy(commit_freq, &baseline_freqs))
		.collect::<Vec<_>>();

	// Sort the commits	by entropy score
	// PANIC: It is safe to unwrap here, because the entropy scores will always be valid floating point numbers if we get to this point
	commit_entropies.sort_by(|a, b| b.entropy.partial_cmp(&a.entropy).unwrap());

	// Convert to Z-scores and return results.
	let entropy_output = z_scores(commit_entropies)
		.map(EntropyOutput::new)
		.map(Arc::new)?;

	log::info!("completed entropy metric");

	Ok(entropy_output)
}

/// The final output for entropy metric, containing an entropy score for
/// every commit.
#[derive(Debug, Eq, PartialEq, Serialize)]
pub struct EntropyOutput {
	/// The set of commit entropies.
	pub commit_entropies: Vec<CommitEntropy>,
}

impl EntropyOutput {
	/// Construct an `Output` from a set of commit entropies.
	fn new(commit_entropies: Vec<CommitEntropy>) -> EntropyOutput {
		EntropyOutput { commit_entropies }
	}
}

/// The entropy of a single commit.
#[derive(Debug, Eq, PartialEq, Serialize)]
pub struct CommitEntropy {
	/// The commit
	pub commit: Arc<Commit>,
	/// The entropy score
	pub entropy: F64,
}

/// The grapheme frequencies of a single commit.
#[derive(Debug)]
struct CommitGraphemeFreq {
	/// The commit.
	commit: Arc<Commit>,
	/// The set of grapheme frequencies.
	grapheme_freqs: Vec<GraphemeFreq>,
}

/// The frequency of a single grapheme.
#[derive(Debug)]
struct GraphemeFreq {
	/// The grapheme.
	grapheme: String,
	/// The frequency.
	freq: f64,
}

impl GraphemeFreq {
	fn as_view(&self) -> GraphemeFreqView<'_> {
		GraphemeFreqView {
			grapheme: &self.grapheme,
			freq: self.freq,
		}
	}
}

/// A view of a grapheme frequency.
struct GraphemeFreqView<'gra> {
	/// The view of the grapheme.
	grapheme: &'gra str,
	/// The freq (fine to copy)
	freq: f64,
}

/// Check if a commit diff is a likely source file.
fn is_likely_source_file(
	commit_diff: &CommitDiff,
	db: &dyn MetricProvider,
) -> crate::error::Result<bool> {
	commit_diff
		.diff
		.file_diffs
		.iter()
		.try_any(|fd| db.is_likely_source_file(Arc::clone(&fd.file_name)))
}

/// Calculate grapheme frequencies for each commit.
fn grapheme_freqs(commit_diff: &CommitDiff, db: &dyn MetricProvider) -> Result<CommitGraphemeFreq> {
	// Dashmap (fast concurrent hashmap) to store counts for each grapheme.
	let grapheme_table: DashMap<String, u64> = DashMap::new();

	// Use this variable to track the total number of graphemes accross all patches in this commit diff.
	let mut total_graphemes: usize = 0;

	// Iterate over the file diffs by reference.
	for file_diff in &commit_diff.diff.file_diffs {
		// Filter out any that are probably not source files.
		if db
			.is_likely_source_file(Arc::clone(&file_diff.file_name))?
			.not()
		{
			continue;
		}

		// Filter out any that are empty.
		if file_diff.patch.is_empty() {
			continue;
		}

		// Count the number of graphemes in this patch, add it to the tortal,
		// and track the number of each grapheme.
		total_graphemes += file_diff
			.patch
			// Iterate in parallel over the lines of the patch.
			.par_lines()
			// Normalize each line.
			// See https://en.wikipedia.org/wiki/Unicode_equivalence.
			.map(|line: &str| line.chars().nfc().collect::<String>())
			// Count the graphemes in each normalized line.
			// Also update the graphemes table here.
			// We'll sum these counts to get the total number of graphemes.
			.map(|normalized_line: String| {
				// Create an iterator over the graphemes in the line.
				Graphemes::new(&normalized_line)
					// Update the graphemes table.
					.map(|grapheme: &str| {
						// Use this if statement to avoid allocating a new string unless needed.
						if let Some(mut count) = grapheme_table.get_mut(grapheme) {
							*count += 1;
						} else {
							grapheme_table.insert(grapheme.to_owned(), 1);
						}
					})
					// get the grapheme count for this normalized line.
					.count()
			})
			.sum::<usize>();
	}

	// Transform out table (dashmap) of graphemes and their frequencies into a list to return.
	let grapheme_freqs = grapheme_table
		// Iterate in parallel for performance.
		.into_par_iter()
		.map(|(grapheme, count)| GraphemeFreq {
			grapheme,
			freq: count as f64 / total_graphemes as f64,
		})
		.collect();

	// Return the collected list of graphemes and their frequencies for this commit diff.
	Ok(CommitGraphemeFreq {
		commit: Arc::clone(&commit_diff.commit),
		grapheme_freqs,
	})
}

/// Calculate baseline frequencies for each grapheme across all commits.
fn baseline_freqs(commit_freqs: &[CommitGraphemeFreq]) -> HashMap<&str, (f64, i64)> {
	let grapheme_freqs = commit_freqs
		.iter()
		.flat_map(|cf| cf.grapheme_freqs.iter().map(GraphemeFreq::as_view));

	let mut baseline = HashMap::new();

	for grapheme_freq in grapheme_freqs {
		let entry = baseline.entry(grapheme_freq.grapheme).or_insert((0.0, 0));
		let cum_avg = entry.0;
		let n = entry.1;
		entry.0 = (grapheme_freq.freq + (n as f64) * cum_avg) / ((n + 1) as f64);
		entry.1 = n + 1;
	}

	baseline
}

/// Calculate commit entropy for each commit.
fn commit_entropy(
	commit_freq: &CommitGraphemeFreq,
	baseline: &HashMap<&str, (f64, i64)>,
) -> CommitEntropy {
	let commit = Arc::clone(&commit_freq.commit);
	let entropy = F64::new(
		commit_freq
			.grapheme_freqs
			.iter()
			.map(|grapheme_freq| {
				// Get the freq for the current commit & grapheme.
				let freq = grapheme_freq.freq;

				// Get the baseline freq for that grapheme across all commits.
				let grapheme = grapheme_freq.grapheme.as_str();
				let baseline_freq = baseline.get(grapheme).unwrap().0;

				// Calculate the score for that grapheme.
				freq * (freq / baseline_freq).log2()
			})
			// Sum all individual grapheme scores together to get the commit's entropy.
			.sum(),
	)
	.unwrap();

	CommitEntropy { commit, entropy }
}

/// Convert entropy scores to Z-scores of the underlying metric.
fn z_scores(mut commit_entropies: Vec<CommitEntropy>) -> Result<Vec<CommitEntropy>> {
	let entropies: Vec<_> = commit_entropies
		.iter()
		.map(|c| c.entropy.into_inner())
		.collect();

	let mean =
		mean(&entropies).ok_or_else(|| crate::error::Error::msg("failed to get mean entropy"))?;
	let std_dev = std_dev(mean, &entropies)
		.ok_or_else(|| crate::error::Error::msg("failed to get entropy standard deviation"))?;

	if std_dev == 0.0 {
		return Err(hc_error!("not enough commits to calculate entropy"));
	}

	for commit_entropy in &mut commit_entropies {
		commit_entropy.entropy = (commit_entropy.entropy - mean) / std_dev;
	}

	Ok(commit_entropies)
}
