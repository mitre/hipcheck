// SPDX-License-Identifier: Apache-2.0

use crate::{error::*, hc_error, types::*};
use dashmap::DashMap;
use finl_unicode::grapheme_clusters::Graphemes;
use rayon::prelude::*;
use std::{
	collections::{HashMap, HashSet},
	iter::Iterator,
	ops::Not,
};
use unicode_normalization::UnicodeNormalization;

/// Calculate the arithmetic mean for a set of floats. Returns an option to account
/// for the possibility of dividing by zero.
pub fn mean(data: &[f64]) -> Option<f64> {
	// Do not use rayon's parallel iter/sum here due to the non-associativity of floating point numbers/math.
	// See: https://en.wikipedia.org/wiki/Associative_property#Nonassociativity_of_floating_point_calculation.
	let sum = data.iter().sum::<f64>();
	let count = data.len();

	match count {
		positive if positive > 0 => Some(sum / count as f64),
		_ => None,
	}
}

/// Calculate the standard deviation for a set of floats. Returns an option to
/// account for the possibility of dividing by zero.
pub fn std_dev(mean: f64, data: &[f64]) -> Option<f64> {
	match (mean, data.len()) {
		(mean, count) if count > 0 => {
			let variance =
				data.iter()
					.map(|value| {
						let diff = mean - *value;
						diff * diff
					})
					.sum::<f64>() / count as f64;

			Some(variance.sqrt())
		}
		_ => None,
	}
}

/// Calculate grapheme frequencies for each commit.
pub fn grapheme_freqs(
	source_files: &HashSet<String>,
	commit_diff: &CommitDiff,
) -> CommitGraphemeFreq {
	// #[cfg(feature = "print-timings")]
	// let _0 = crate::benchmarking::print_scope_time!("grapheme_freqs");

	// Dashmap (fast concurrent hashmap) to store counts for each grapheme.
	let grapheme_table: DashMap<String, u64> = DashMap::new();

	// Use this variable to track the total number of graphemes accross all patches in this commit diff.
	let tgt_diffs: Vec<&FileDiff> = commit_diff
		.diff
		.file_diffs
		.iter()
		.filter(|file_diff| {
			// Filter out any that are probably not source files, or are empty patches
			source_files.contains(&file_diff.file_name) && file_diff.patch.is_empty().not()
		})
		.collect();
	// Use this variable to track the total number of graphemes accross all patches in this commit diff.
	let total_graphemes: usize = tgt_diffs
		// Iterate over each line of each file in parallel
		.par_iter()
		.flat_map(|file_diff| file_diff.patch.par_lines())
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
		// Aggregate the grapheme count across all lines of all files
		.sum();

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
	CommitGraphemeFreq {
		commit: commit_diff.commit.clone(),
		grapheme_freqs,
	}
}

/// Calculate baseline frequencies for each grapheme across all commits.
pub fn baseline_freqs(commit_freqs: &[CommitGraphemeFreq]) -> HashMap<&str, (f64, i64)> {
	// PERFORMANCE: At the moment this function appears to be faster single-threaded.
	// I tried switching out the hashmap with a Dashamp and switching the iterator to rayon,
	// but the overhead is not worth it (against express we go from 3 milliseconds to 6).
	// This may be worth revisiting if we prioritize projects with huge numbers of commits, but at the moment
	// I will leave it be.

	let mut baseline: HashMap<&str, (f64, i64)> = HashMap::new();

	commit_freqs
		.iter()
		.flat_map(|cf: &CommitGraphemeFreq| cf.grapheme_freqs.iter().map(GraphemeFreq::as_view))
		.for_each(|view: GraphemeFreqView| {
			let entry = baseline.entry(view.grapheme).or_insert((0.0, 0));
			let cum_avg = entry.0;
			let n = entry.1;
			entry.0 = (view.freq + (n as f64) * cum_avg) / ((n + 1) as f64);
			entry.1 = n + 1;
		});

	baseline
}

/// Calculate commit entropy for each commit.
pub fn commit_entropy(
	commit_freq: &CommitGraphemeFreq,
	baseline: &HashMap<&str, (f64, i64)>,
) -> CommitEntropy {
	let commit = commit_freq.commit.clone();
	let entropy = commit_freq
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
		.sum();

	CommitEntropy { commit, entropy }
}

/// Convert entropy scores to Z-scores of the underlying metric.
pub fn z_scores(mut commit_entropies: Vec<CommitEntropy>) -> Result<Vec<CommitEntropy>> {
	let entropies: Vec<_> = commit_entropies.iter().map(|c| c.entropy).collect();

	let mean = mean(&entropies).ok_or_else(|| hc_error!("failed to get mean entropy"))?;
	let std_dev = std_dev(mean, &entropies)
		.ok_or_else(|| hc_error!("failed to get entropy standard deviation"))?;

	if std_dev == 0.0 {
		return Err(hc_error!("not enough commits to calculate entropy"));
	}

	for commit_entropy in &mut commit_entropies {
		commit_entropy.entropy = (commit_entropy.entropy - mean) / std_dev;
	}

	Ok(commit_entropies)
}
