// SPDX-License-Identifier: Apache-2.0

pub mod report_builder;
pub mod result;
pub mod score;

use crate::{
	config::{AttacksConfigQuery, CommitConfigQuery, PracticesConfigQuery},
	data::git::GitProvider,
	error::{Error, Result},
	metric::{affiliation::AffiliatedType, MetricProvider},
	plugin::QueryResult,
	report::Concern,
	F64,
};
use std::{
	collections::{HashMap, HashSet},
	default::Default,
	fmt,
	fmt::{Display, Formatter},
	ops::Not,
	sync::Arc,
};

/// Queries about analyses
#[salsa::query_group(AnalysisProviderStorage)]
pub trait AnalysisProvider:
	AttacksConfigQuery + CommitConfigQuery + GitProvider + MetricProvider + PracticesConfigQuery
{
	/// Returns result of activity analysis
	fn activity_analysis(&self) -> Result<QueryResult>;

	/// Returns result of affiliation analysis
	fn affiliation_analysis(&self) -> Result<QueryResult>;

	/// Returns result of binary analysis
	fn binary_analysis(&self) -> Result<QueryResult>;

	/// Returns result of churn analysis
	fn churn_analysis(&self) -> Result<QueryResult>;

	/// Returns result of entropy analysis
	fn entropy_analysis(&self) -> Result<QueryResult>;

	/// Returns result of identity analysis
	fn identity_analysis(&self) -> Result<QueryResult>;

	/// Returns result of fuzz analysis
	fn fuzz_analysis(&self) -> Result<QueryResult>;

	/// Returns result of review analysis
	fn review_analysis(&self) -> Result<QueryResult>;

	/// Returns result of typo analysis
	fn typo_analysis(&self) -> Result<QueryResult>;
}

#[allow(unused)]
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

#[allow(unused)]
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

pub fn activity_analysis(db: &dyn AnalysisProvider) -> Result<QueryResult> {
	let results = db.activity_metric()?;
	let value = results.time_since_last_commit.num_weeks() as u64;
	Ok(QueryResult {
		value: serde_json::to_value(value)?,
		concerns: vec![],
	})
}

pub fn affiliation_analysis(db: &dyn AnalysisProvider) -> Result<QueryResult> {
	let results = db.affiliation_metric()?;

	let affiliated_iter = results
		.affiliations
		.iter()
		.filter(|a| a.affiliated_type.is_affiliated());

	// @Note - policy expr json injection can't handle objs/strings currently
	let value: Vec<bool> = affiliated_iter.clone().map(|_| true).collect();

	let mut contributor_freq_map = HashMap::new();

	for affiliation in affiliated_iter {
		let commit_view = db.contributors_for_commit(Arc::clone(&affiliation.commit))?;

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

	let concerns: Vec<String> = contributor_freq_map
		.into_iter()
		.map(|(contributor, count)| format!("Contributor {} has count {}", contributor, count))
		.collect();

	Ok(QueryResult {
		value: serde_json::to_value(value)?,
		concerns,
	})
}

pub fn binary_analysis(db: &dyn AnalysisProvider) -> Result<QueryResult> {
	let results = db.binary_metric()?;
	let concerns: Vec<String> = results
		.binary_files
		.iter()
		.map(|binary_file| format!("Binary file at '{}'", binary_file))
		.collect();
	Ok(QueryResult {
		value: serde_json::to_value(&results.binary_files)?,
		concerns,
	})
}

pub fn churn_analysis(db: &dyn AnalysisProvider) -> Result<QueryResult> {
	let results = db.churn_metric()?;
	let value: Vec<F64> = results.commit_churn_freqs.iter().map(|o| o.churn).collect();
	// @Todo - in RFD4 transition we lost the ability to flag commits, because
	// the need to flag them as concerns is dependent on policy expr
	Ok(QueryResult {
		value: serde_json::to_value(value)?,
		concerns: vec![],
	})
}

pub fn entropy_analysis(db: &dyn AnalysisProvider) -> Result<QueryResult> {
	let results = db.entropy_metric()?;
	let value: Vec<F64> = results.commit_entropies.iter().map(|o| o.entropy).collect();
	// @Todo - in RFD4 transition we lost the ability to flag commits, because
	// the need to flag them as concerns is dependent on policy expr
	Ok(QueryResult {
		value: serde_json::to_value(value)?,
		concerns: vec![],
	})
}

pub fn identity_analysis(db: &dyn AnalysisProvider) -> Result<QueryResult> {
	let results = db.identity_metric()?;
	let num_flagged = results
		.matches
		.iter()
		.filter(|m| m.identities_match)
		.count() as u64;
	let percent_flagged = num_flagged as f64 / results.matches.len() as f64;
	let value = F64::new(percent_flagged).expect("Percent threshold should never be NaN");
	Ok(QueryResult {
		value: serde_json::to_value(value)?,
		concerns: vec![],
	})
}

pub fn fuzz_analysis(db: &dyn AnalysisProvider) -> Result<QueryResult> {
	let results = db.fuzz_metric()?;
	let value = results.fuzz_result.exists;
	Ok(QueryResult {
		value: serde_json::to_value(value)?,
		concerns: vec![],
	})
}

pub fn review_analysis(db: &dyn AnalysisProvider) -> Result<QueryResult> {
	let results = db.review_metric()?;
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
	Ok(QueryResult {
		value: serde_json::to_value(value)?,
		concerns: vec![],
	})
}

pub fn typo_analysis(db: &dyn AnalysisProvider) -> Result<QueryResult> {
	let results = db.typo_metric()?;

	// @Note - policy expr json injection does not support string/obj as array elts
	let value = results.typos.iter().map(|_| true).collect::<Vec<bool>>();

	let concerns: Vec<String> = results
		.typos
		.iter()
		.map(|typodep| typodep.dependency.to_string())
		.collect::<HashSet<_>>()
		.into_iter()
		.collect();

	Ok(QueryResult {
		value: serde_json::to_value(value)?,
		concerns,
	})
}

#[allow(unused)]
fn score_by_threshold<T: PartialOrd>(value: T, threshold: T) -> i64 {
	if value > threshold {
		1
	} else {
		0
	}
}

#[allow(unused)]
fn score_by_threshold_reversed<T: PartialOrd>(value: T, threshold: T) -> i64 {
	if value >= threshold {
		0
	} else {
		1
	}
}
