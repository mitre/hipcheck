// SPDX-License-Identifier: Apache-2.0

use crate::analysis::result::*;
use crate::config::AttacksConfigQuery;
use crate::config::CommitConfigQuery;
use crate::config::FuzzConfigQuery;
use crate::config::PracticesConfigQuery;
use crate::data::git::GitProvider;
use crate::error::Error;
use crate::error::Result;
use crate::metric::affiliation::AffiliatedType;
use crate::metric::MetricProvider;
use crate::report::Concern;
use crate::F64;
use std::collections::HashMap;
use std::collections::HashSet;
use std::default::Default;
use std::fmt;
use std::fmt::Display;
use std::fmt::Formatter;
use std::ops::Not;
use std::sync::Arc;

/// Queries about analyses
#[salsa::query_group(AnalysisProviderStorage)]
pub trait AnalysisProvider:
	AttacksConfigQuery
	+ CommitConfigQuery
	+ GitProvider
	+ MetricProvider
	+ FuzzConfigQuery
	+ PracticesConfigQuery
{
	/// Returns result of activity analysis
	fn activity_analysis(&self) -> Arc<HCAnalysisReport>;

	/// Returns result of affiliation analysis
	fn affiliation_analysis(&self) -> Arc<HCAnalysisReport>;

	/// Returns result of binary analysis
	fn binary_analysis(&self) -> Arc<HCAnalysisReport>;

	/// Returns result of churn analysis
	fn churn_analysis(&self) -> Arc<HCAnalysisReport>;

	/// Returns result of entropy analysis
	fn entropy_analysis(&self) -> Arc<HCAnalysisReport>;

	/// Returns result of identity analysis
	fn identity_analysis(&self) -> Arc<HCAnalysisReport>;

	/// Returns result of fuzz analysis
	fn fuzz_analysis(&self) -> Arc<HCAnalysisReport>;

	/// Returns result of review analysis
	fn review_analysis(&self) -> Arc<HCAnalysisReport>;

	/// Returns result of typo analysis
	fn typo_analysis(&self) -> Arc<HCAnalysisReport>;
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum AnalysisReport {
	/// Affiliation analysis result.
	Affiliation {
		value: u64,
		threshold: u64,
		outcome: AnalysisOutcome,
		concerns: Vec<Concern>,
	},
	/// Binary file analysis result.
	Binary {
		value: u64,
		threshold: u64,
		outcome: AnalysisOutcome,
		concerns: Vec<Concern>,
	},
	/// Churn analysis result.
	Churn {
		value: F64,
		threshold: F64,
		outcome: AnalysisOutcome,
		concerns: Vec<Concern>,
	},
	/// Entropy analysis result.
	Entropy {
		value: F64,
		threshold: F64,
		outcome: AnalysisOutcome,
		concerns: Vec<Concern>,
	},
	/// Identity analysis result.
	Identity {
		value: F64,
		threshold: F64,
		outcome: AnalysisOutcome,
		concerns: Vec<Concern>,
	},
	/// Fuzz repo analysis result.
	Fuzz {
		value: bool,
		outcome: AnalysisOutcome,
		concerns: Vec<Concern>,
	},
	/// Review analysis result.
	Review {
		value: F64,
		threshold: F64,
		outcome: AnalysisOutcome,
		concerns: Vec<Concern>,
	},
	/// Typo analysis result.
	Typo {
		value: u64,
		threshold: u64,
		outcome: AnalysisOutcome,
		concerns: Vec<Concern>,
	},
	/// "Result" for a skipped or errored analysis
	None { outcome: AnalysisOutcome },
}

impl Default for AnalysisReport {
	fn default() -> AnalysisReport {
		AnalysisReport::None {
			outcome: AnalysisOutcome::Skipped,
		}
	}
}

#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub enum AnalysisOutcome {
	#[default]
	Skipped,
	Error(Error),
	Pass(String),
	Fail(String),
}

impl Display for AnalysisOutcome {
	fn fmt(&self, f: &mut Formatter) -> fmt::Result {
		match self {
			AnalysisOutcome::Skipped => write!(f, "SKIPPED"),
			AnalysisOutcome::Error(msg) => write!(f, "ERROR   {}", msg),
			AnalysisOutcome::Pass(msg) => write!(f, "PASS   {}", msg),
			AnalysisOutcome::Fail(msg) => write!(f, "FAIL   {}", msg),
		}
	}
}

pub fn activity_analysis(db: &dyn AnalysisProvider) -> Arc<HCAnalysisReport> {
	let results = match db.activity_metric() {
		Err(err) => return Arc::new(HCAnalysisReport::generic_error(err, vec![])),
		Ok(results) => results,
	};
	let value = results.time_since_last_commit.num_weeks() as u64;
	Arc::new(HCAnalysisReport {
		outcome: HCAnalysisOutcome::Completed(HCAnalysisValue::Basic(value.into())),
		concerns: vec![],
	})
}

pub fn affiliation_analysis(db: &dyn AnalysisProvider) -> Arc<HCAnalysisReport> {
	let results = match db.affiliation_metric() {
		Err(err) => return Arc::new(HCAnalysisReport::generic_error(err, vec![])),
		Ok(results) => results,
	};

	let affiliated_iter = results
		.affiliations
		.iter()
		.filter(|a| a.affiliated_type.is_affiliated());

	let value = affiliated_iter.clone().count() as u64;

	let mut contributor_freq_map = HashMap::new();

	for affiliation in affiliated_iter {
		let commit_view = match db.contributors_for_commit(Arc::clone(&affiliation.commit)) {
			Err(err) => return Arc::new(HCAnalysisReport::generic_error(err, vec![])),
			Ok(cv) => cv,
		};

		let contributor = match affiliation.affiliated_type {
			AffiliatedType::Author => String::from(&commit_view.author.name),
			AffiliatedType::Committer => String::from(&commit_view.committer.name),
			AffiliatedType::Neither => String::from("Neither"),
			AffiliatedType::Both => String::from("Both"),
		};

		let count_commits_for = |contributor| {
			db.commits_for_contributor(Arc::clone(contributor))
				.into_iter()
				.count() as i64
		};

		let author_commits = count_commits_for(&commit_view.author);
		let committer_commits = count_commits_for(&commit_view.committer);

		let commit_count = match affiliation.affiliated_type {
			AffiliatedType::Neither => 0,
			AffiliatedType::Both => author_commits + committer_commits,
			AffiliatedType::Author => author_commits,
			AffiliatedType::Committer => committer_commits,
		};

		// Add string representation of affiliated contributor with count of associated commits
		contributor_freq_map.insert(contributor, commit_count);
	}

	let concerns = contributor_freq_map
		.into_iter()
		.map(|(contributor, count)| Concern::Affiliation { contributor, count })
		.collect();

	Arc::new(HCAnalysisReport {
		outcome: HCAnalysisOutcome::Completed(HCAnalysisValue::Basic(value.into())),
		concerns,
	})
}

pub fn binary_analysis(db: &dyn AnalysisProvider) -> Arc<HCAnalysisReport> {
	let results = match db.binary_metric() {
		Err(err) => return Arc::new(HCAnalysisReport::generic_error(err, vec![])),
		Ok(results) => results,
	};
	let value = results.binary_files.len() as u64;
	let concerns = results
		.binary_files
		.clone()
		.into_iter()
		.map(|binary_file| Concern::Binary {
			file_path: binary_file.as_ref().to_string(),
		})
		.collect();
	Arc::new(HCAnalysisReport {
		outcome: HCAnalysisOutcome::Completed(HCAnalysisValue::Basic(value.into())),
		concerns,
	})
}

pub fn churn_analysis(db: &dyn AnalysisProvider) -> Arc<HCAnalysisReport> {
	let results = match db.churn_metric() {
		Err(err) => return Arc::new(HCAnalysisReport::generic_error(err, vec![])),
		Ok(results) => results,
	};
	let value_threshold = *db.churn_value_threshold();
	let num_flagged = results
		.commit_churn_freqs
		.iter()
		.filter(|c| c.churn.into_inner() > value_threshold)
		.count() as u64;
	let percent_flagged = num_flagged as f64 / results.commit_churn_freqs.len() as f64;
	let value = F64::new(percent_flagged).expect("Percent threshold should never be NaN");
	let concerns = results
		.commit_churn_freqs
		.iter()
		.filter(|c| c.churn.into_inner() > value_threshold)
		.map(|cf| Concern::Churn {
			commit_hash: cf.commit.hash.clone(),
			score: cf.churn.into_inner(),
			threshold: value_threshold,
		})
		.collect::<Vec<_>>();
	Arc::new(HCAnalysisReport {
		outcome: HCAnalysisOutcome::Completed(HCAnalysisValue::Basic(value.into())),
		concerns,
	})
}

pub fn entropy_analysis(db: &dyn AnalysisProvider) -> Arc<HCAnalysisReport> {
	let results = match db.entropy_metric() {
		Err(err) => return Arc::new(HCAnalysisReport::generic_error(err, vec![])),
		Ok(results) => results,
	};
	let value_threshold = *db.entropy_value_threshold();
	let num_flagged = results
		.commit_entropies
		.iter()
		.filter(|c| c.entropy.into_inner() > value_threshold)
		.count() as u64;
	let percent_flagged = num_flagged as f64 / results.commit_entropies.len() as f64;

	let value = F64::new(percent_flagged).expect("Percent threshold should never be NaN");
	let res_concerns = results
		.commit_entropies
		.iter()
		.filter(|c| c.entropy.into_inner() > value_threshold)
		.map(|cf| {
			db.get_short_hash(Arc::new(cf.commit.hash.clone()))
				.map(|commit_hash| Concern::Entropy {
					commit_hash: commit_hash.trim().to_owned(),
					score: cf.entropy.into_inner(),
					threshold: value_threshold,
				})
		})
		.collect::<Result<Vec<_>>>();
	let concerns = match res_concerns {
		Ok(c) => c,
		Err(err) => return Arc::new(HCAnalysisReport::generic_error(err, vec![])),
	};

	Arc::new(HCAnalysisReport {
		outcome: HCAnalysisOutcome::Completed(HCAnalysisValue::Basic(value.into())),
		concerns,
	})
}

pub fn identity_analysis(db: &dyn AnalysisProvider) -> Arc<HCAnalysisReport> {
	let results = match db.identity_metric() {
		Err(err) => return Arc::new(HCAnalysisReport::generic_error(err, vec![])),
		Ok(results) => results,
	};
	let num_flagged = results
		.matches
		.iter()
		.filter(|m| m.identities_match)
		.count() as u64;
	let percent_flagged = num_flagged as f64 / results.matches.len() as f64;
	let value = F64::new(percent_flagged).expect("Percent threshold should never be NaN");

	Arc::new(HCAnalysisReport {
		outcome: HCAnalysisOutcome::Completed(HCAnalysisValue::Basic(value.into())),
		concerns: vec![],
	})
}

pub fn fuzz_analysis(db: &dyn AnalysisProvider) -> Arc<HCAnalysisReport> {
	let results = match db.fuzz_metric() {
		Err(err) => return Arc::new(HCAnalysisReport::generic_error(err, vec![])),
		Ok(results) => results,
	};
	let exists = results.fuzz_result.exists;

	Arc::new(HCAnalysisReport {
		outcome: HCAnalysisOutcome::Completed(HCAnalysisValue::Basic(exists.into())),
		concerns: vec![],
	})
}

pub fn review_analysis(db: &dyn AnalysisProvider) -> Arc<HCAnalysisReport> {
	let results = match db.review_metric() {
		Err(err) => return Arc::new(HCAnalysisReport::generic_error(err, vec![])),
		Ok(results) => results,
	};
	let num_flagged = results
		.pull_reviews
		.iter()
		.filter(|p| p.has_review.not())
		.count() as u64;

	let percent_flagged = match (num_flagged, results.pull_reviews.len()) {
		(flagged, total) if flagged != 0 && total != 0 => {
			num_flagged as f64 / results.pull_reviews.len() as f64
		}
		_ => 0.0,
	};
	let value = F64::new(percent_flagged).expect("Percent threshold should never be NaN");

	Arc::new(HCAnalysisReport {
		outcome: HCAnalysisOutcome::Completed(HCAnalysisValue::Basic(value.into())),
		concerns: vec![],
	})
}

pub fn typo_analysis(db: &dyn AnalysisProvider) -> Arc<HCAnalysisReport> {
	let results = match db.typo_metric() {
		Err(err) => return Arc::new(HCAnalysisReport::generic_error(err, vec![])),
		Ok(results) => results,
	};
	let num_flagged = results.typos.len() as u64;

	let concerns: Vec<_> = results
		.typos
		.iter()
		.map(|typodep| Concern::Typo {
			dependency_name: typodep.dependency.to_string(),
		})
		.collect::<HashSet<_>>()
		.into_iter()
		.collect();

	Arc::new(HCAnalysisReport {
		outcome: HCAnalysisOutcome::Completed(HCAnalysisValue::Basic(num_flagged.into())),
		concerns,
	})
}

fn score_by_threshold<T: PartialOrd>(value: T, threshold: T) -> i64 {
	if value > threshold {
		1
	} else {
		0
	}
}

fn score_by_threshold_reversed<T: PartialOrd>(value: T, threshold: T) -> i64 {
	if value >= threshold {
		0
	} else {
		1
	}
}
