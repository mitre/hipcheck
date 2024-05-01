// SPDX-License-Identifier: Apache-2.0

use hc_common::{
	chrono::Duration,
	error::{Error, Result},
	salsa, F64,
};
use hc_config::{AttacksConfigQuery, CommitConfigQuery, FuzzConfigQuery, PracticesConfigQuery};
use hc_git::GitProvider;
use hc_metric::{affiliation::AffiliatedType, MetricProvider};
use hc_report::{Concern, PrConcern};
use std::collections::{HashMap, HashSet};
use std::default::Default;
use std::fmt::{self, Display, Formatter};
use std::ops::Not;
use std::rc::Rc;

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
	fn activity_analysis(&self) -> Result<Rc<AnalysisReport>>;

	/// Returns result of affiliation analysis
	fn affiliation_analysis(&self) -> Result<Rc<AnalysisReport>>;

	/// Returns result of binary analysis
	fn binary_analysis(&self) -> Result<Rc<AnalysisReport>>;

	/// Returns result of churn analysis
	fn churn_analysis(&self) -> Result<Rc<AnalysisReport>>;

	/// Returns result of entropy analysis
	fn entropy_analysis(&self) -> Result<Rc<AnalysisReport>>;

	/// Returns result of identity analysis
	fn identity_analysis(&self) -> Result<Rc<AnalysisReport>>;

	/// Returns result of fuzz analysis
	fn fuzz_analysis(&self) -> Result<Rc<AnalysisReport>>;

	/// Returns result of review analysis
	fn review_analysis(&self) -> Result<Rc<AnalysisReport>>;

	/// Returns result of typo analysis
	fn typo_analysis(&self) -> Result<Rc<AnalysisReport>>;

	/// Returns result of pull request affiliation analysis
	fn pr_affiliation_analysis(&self) -> Result<Rc<AnalysisReport>>;

	/// Returns result of pull request contributor trust analysis
	fn pr_contributor_trust_analysis(&self) -> Result<Rc<AnalysisReport>>;

	/// Returns result of pull request module contributors analysis
	fn pr_module_contributors_analysis(&self) -> Result<Rc<AnalysisReport>>;
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum AnalysisReport {
	/// Activity analysis result.
	Activity {
		value: u64,
		threshold: u64,
		outcome: AnalysisOutcome,
		concerns: Vec<Concern>,
	},
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
	/// Pull request affiliation analysis result.
	PrAffiliation {
		value: u64,
		threshold: u64,
		outcome: AnalysisOutcome,
		concerns: Vec<PrConcern>,
	},
	/// Pull request contributor trust analysis result.
	PrContributorTrust {
		value: F64,
		threshold: F64,
		outcome: AnalysisOutcome,
		concerns: Vec<PrConcern>,
	},
	/// Pull request module contributor analysis result.
	PrModuleContributors {
		value: F64,
		threshold: F64,
		outcome: AnalysisOutcome,
		concerns: Vec<PrConcern>,
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

pub fn activity_analysis(db: &dyn AnalysisProvider) -> Result<Rc<AnalysisReport>> {
	if db.activity_active() {
		let results = db.activity_metric();
		match results {
			Err(err) => Ok(Rc::new(AnalysisReport::None {
				outcome: AnalysisOutcome::Error(err),
			})),
			Ok(results) => {
				let value = results.time_since_last_commit.num_weeks();
				let threshold =
					Duration::weeks(db.activity_week_count_threshold() as i64).num_weeks();
				let results_score = score_by_threshold(value, threshold);

				let concerns = Vec::new();

				if results_score == 0 {
					let msg = format!(
						"{} weeks inactivity <= {} weeks inactivity",
						value, threshold
					);
					Ok(Rc::new(AnalysisReport::Activity {
						value: value as u64,
						threshold: threshold as u64,
						outcome: AnalysisOutcome::Pass(msg),
						concerns,
					}))
				} else {
					let msg = format!(
						"{} weeks inactivity > {} weeks inactivity",
						value, threshold
					);
					Ok(Rc::new(AnalysisReport::Activity {
						value: value as u64,
						threshold: threshold as u64,
						outcome: AnalysisOutcome::Fail(msg),
						concerns,
					}))
				}
			}
		}
	} else {
		Ok(Rc::new(AnalysisReport::None {
			outcome: AnalysisOutcome::Skipped,
		}))
	}
}

pub fn affiliation_analysis(db: &dyn AnalysisProvider) -> Result<Rc<AnalysisReport>> {
	if db.affiliation_active() {
		let results = db.affiliation_metric();

		match results {
			Err(err) => Ok(Rc::new(AnalysisReport::None {
				outcome: AnalysisOutcome::Error(err),
			})),
			Ok(results) => {
				let affiliated_iter = results
					.affiliations
					.iter()
					.filter(|a| a.affiliated_type.is_affiliated());

				let value = affiliated_iter.clone().count() as u64;
				let threshold = db.affiliation_count_threshold();
				let results_score = score_by_threshold(value, threshold);

				let mut contributor_freq_map = HashMap::new();

				for affiliation in affiliated_iter {
					let commit_view = db.contributors_for_commit(Rc::clone(&affiliation.commit))?;

					let contributor = match affiliation.affiliated_type {
						AffiliatedType::Author => String::from(&commit_view.author.name),
						AffiliatedType::Committer => String::from(&commit_view.committer.name),
						AffiliatedType::Neither => String::from("Neither"),
						AffiliatedType::Both => String::from("Both"),
					};

					let count_commits_for = |contributor| {
						db.commits_for_contributor(Rc::clone(contributor))
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

				if results_score == 0 {
					let msg = format!("{} affiliated <= {} affiliated", value, threshold);
					Ok(Rc::new(AnalysisReport::Affiliation {
						value,
						threshold,
						outcome: AnalysisOutcome::Pass(msg),
						concerns,
					}))
				} else {
					let msg = format!("{} affiliated > {} affiliated", value, threshold);
					Ok(Rc::new(AnalysisReport::Affiliation {
						value,
						threshold,
						outcome: AnalysisOutcome::Fail(msg),
						concerns,
					}))
				}
			}
		}
	} else {
		Ok(Rc::new(AnalysisReport::None {
			outcome: AnalysisOutcome::Skipped,
		}))
	}
}

pub fn binary_analysis(db: &dyn AnalysisProvider) -> Result<Rc<AnalysisReport>> {
	if db.binary_active() {
		let results = db.binary_metric();

		match results {
			Err(err) => Ok(Rc::new(AnalysisReport::None {
				outcome: AnalysisOutcome::Error(err),
			})),
			Ok(results) => {
				let value = results.binary_files.len() as u64;
				let threshold = db.binary_count_threshold();
				let results_score = score_by_threshold(value, threshold);

				let concerns = results
					.binary_files
					.clone()
					.into_iter()
					.map(|binary_file| Concern::Binary {
						file_path: binary_file.as_ref().to_string(),
					})
					.collect();

				if results_score == 0 {
					let msg = format!(
						"{} binary files found <= {} binary files found",
						value, threshold
					);
					Ok(Rc::new(AnalysisReport::Binary {
						value,
						threshold,
						outcome: AnalysisOutcome::Pass(msg),
						concerns,
					}))
				} else {
					let msg = format!(
						"{} binary files found >= {} binary files found",
						value, threshold
					);
					Ok(Rc::new(AnalysisReport::Binary {
						value,
						threshold,
						outcome: AnalysisOutcome::Fail(msg),
						concerns,
					}))
				}
			}
		}
	} else {
		Ok(Rc::new(AnalysisReport::None {
			outcome: AnalysisOutcome::Skipped,
		}))
	}
}

pub fn churn_analysis(db: &dyn AnalysisProvider) -> Result<Rc<AnalysisReport>> {
	if db.churn_active() {
		let results = db.churn_metric();

		match results {
			Err(err) => Ok(Rc::new(AnalysisReport::None {
				outcome: AnalysisOutcome::Error(err),
			})),
			Ok(results) => {
				let value_threshold = *db.churn_value_threshold();
				let num_flagged = results
					.commit_churn_freqs
					.iter()
					.filter(|c| c.churn.into_inner() > value_threshold)
					.count() as u64;
				let percent_flagged = num_flagged as f64 / results.commit_churn_freqs.len() as f64;
				let percent_threshold = *db.churn_percent_threshold();
				let results_score = score_by_threshold(percent_flagged, percent_threshold);

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

				if results_score == 0 {
					let msg = format!(
						"{:.2}% over churn threshold <= {:.2}% over churn threshold",
						percent_flagged * 100.0,
						percent_threshold * 100.0
					);
					// PANIC: percent_flagged and percent_threshold will never be NaN
					Ok(Rc::new(AnalysisReport::Churn {
						value: F64::new(percent_flagged)
							.expect("Percent flagged should never be NaN"),
						threshold: F64::new(percent_threshold)
							.expect("Percent threshold should never be NaN"),
						outcome: AnalysisOutcome::Pass(msg),
						concerns,
					}))
				} else {
					let msg = format!(
						"{:.2}% over churn threshold > {:.2}% over churn threshold",
						percent_flagged * 100.0,
						percent_threshold * 100.0
					);
					// PANIC: percent_flagged and percent_threshold will never be NaN
					Ok(Rc::new(AnalysisReport::Churn {
						value: F64::new(percent_flagged)
							.expect("Percent flagged should never be NaN"),
						threshold: F64::new(percent_threshold)
							.expect("Percent threshold should never be NaN"),
						outcome: AnalysisOutcome::Fail(msg),
						concerns,
					}))
				}
			}
		}
	} else {
		Ok(Rc::new(AnalysisReport::None {
			outcome: AnalysisOutcome::Skipped,
		}))
	}
}

pub fn entropy_analysis(db: &dyn AnalysisProvider) -> Result<Rc<AnalysisReport>> {
	if db.entropy_active() {
		let results = db.entropy_metric();

		match results {
			Err(err) => Ok(Rc::new(AnalysisReport::None {
				outcome: AnalysisOutcome::Error(err),
			})),
			Ok(results) => {
				let value_threshold = *db.entropy_value_threshold();
				let num_flagged = results
					.commit_entropies
					.iter()
					.filter(|c| c.entropy.into_inner() > value_threshold)
					.count() as u64;
				let percent_flagged = num_flagged as f64 / results.commit_entropies.len() as f64;
				let percent_threshold = *db.entropy_percent_threshold();
				let results_score = score_by_threshold(percent_flagged, percent_threshold);

				let concerns = results
					.commit_entropies
					.iter()
					.filter(|c| c.entropy.into_inner() > value_threshold)
					.map(|cf| {
						db.get_short_hash(Rc::new(cf.commit.hash.clone()))
							.map(|commit_hash| Concern::Entropy {
								commit_hash: commit_hash.trim().to_owned(),
								score: cf.entropy.into_inner(),
								threshold: value_threshold,
							})
					})
					.collect::<Result<Vec<_>>>()?;

				if results_score == 0 {
					let msg = format!(
						"{:.2}% over entropy threshold <= {:.2}% over entropy threshold",
						percent_flagged * 100.0,
						percent_threshold * 100.0
					);
					// PANIC: percent_flagged and percent_threshold will never be NaN
					Ok(Rc::new(AnalysisReport::Entropy {
						value: F64::new(percent_flagged)
							.expect("Percent flagged should never be NaN"),
						threshold: F64::new(percent_threshold)
							.expect("Percent threshold should never be NaN"),
						outcome: AnalysisOutcome::Pass(msg),
						concerns,
					}))
				} else {
					let msg = format!(
						"{:.2}% over entropy threshold > {:.2}% over entropy threshold",
						percent_flagged * 100.0,
						percent_threshold * 100.0
					);
					// PANIC: percent_flagged and percent_threshold will never be NaN
					Ok(Rc::new(AnalysisReport::Entropy {
						value: F64::new(percent_flagged)
							.expect("Percent flagged should never be NaN"),
						threshold: F64::new(percent_threshold)
							.expect("Percent threshold should never be NaN"),
						outcome: AnalysisOutcome::Fail(msg),
						concerns,
					}))
				}
			}
		}
	} else {
		Ok(Rc::new(AnalysisReport::None {
			outcome: AnalysisOutcome::Skipped,
		}))
	}
}

pub fn identity_analysis(db: &dyn AnalysisProvider) -> Result<Rc<AnalysisReport>> {
	if db.identity_active() {
		let results = db.identity_metric();

		match results {
			Err(err) => Ok(Rc::new(AnalysisReport::None {
				outcome: AnalysisOutcome::Error(err),
			})),
			Ok(results) => {
				let num_flagged = results
					.matches
					.iter()
					.filter(|m| m.identities_match)
					.count() as u64;
				let percent_flagged = num_flagged as f64 / results.matches.len() as f64;
				let percent_threshold = *db.identity_percent_threshold();
				let results_score = score_by_threshold(percent_flagged, percent_threshold);

				let concerns = Vec::new();

				if results_score == 0 {
					let msg = format!(
						"{:.2}% identity match <= {:.2}% identity match",
						percent_flagged * 100.0,
						percent_threshold * 100.0
					);
					// PANIC: percent_flagged and percent_threshold will never be NaN
					Ok(Rc::new(AnalysisReport::Identity {
						value: F64::new(percent_flagged)
							.expect("Percent flagged should never be NaN"),
						threshold: F64::new(percent_threshold)
							.expect("Percent threshold should never be NaN"),
						outcome: AnalysisOutcome::Pass(msg),
						concerns,
					}))
				} else {
					let msg = format!(
						"{:.2}% identity match > {:.2}% identity match",
						percent_flagged * 100.0,
						percent_threshold * 100.0
					);
					// PANIC: percent_flagged and percent_threshold will never be NaN
					Ok(Rc::new(AnalysisReport::Identity {
						value: F64::new(percent_flagged)
							.expect("Percent flagged should never be NaN"),
						threshold: F64::new(percent_threshold)
							.expect("Percent threshold should never be NaN"),
						outcome: AnalysisOutcome::Fail(msg),
						concerns,
					}))
				}
			}
		}
	} else {
		Ok(Rc::new(AnalysisReport::None {
			outcome: AnalysisOutcome::Skipped,
		}))
	}
}

pub fn fuzz_analysis(db: &dyn AnalysisProvider) -> Result<Rc<AnalysisReport>> {
	if db.fuzz_active() {
		let results = db.fuzz_metric();

		match results {
			Err(err) => Ok(Rc::new(AnalysisReport::None {
				outcome: AnalysisOutcome::Error(err),
			})),
			Ok(results) => {
				let exists = results.fuzz_result.exists;

				// If exists is true, meaning it is fuzzed, score is 0
				let results_score: i64 = match exists {
					value if value => 0,
					_ => 1,
				};

				let concerns = Vec::new();

				if results_score == 0 {
					let msg = format!("Is fuzzed: {} results found", exists);
					Ok(Rc::new(AnalysisReport::Fuzz {
						value: exists,
						outcome: AnalysisOutcome::Pass(msg),
						concerns,
					}))
				} else {
					let msg = format!("Is not fuzzed: {} no results found", exists);
					Ok(Rc::new(AnalysisReport::Fuzz {
						value: exists,
						outcome: AnalysisOutcome::Fail(msg),
						concerns,
					}))
				}
			}
		}
	} else {
		Ok(Rc::new(AnalysisReport::None {
			outcome: AnalysisOutcome::Skipped,
		}))
	}
}

pub fn review_analysis(db: &dyn AnalysisProvider) -> Result<Rc<AnalysisReport>> {
	if db.review_active() {
		let results = db.review_metric();

		match results {
			Err(err) => Ok(Rc::new(AnalysisReport::None {
				outcome: AnalysisOutcome::Error(err),
			})),
			Ok(results) => {
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

				let percent_threshold = *db.review_percent_threshold();
				let results_score = score_by_threshold(percent_flagged, percent_threshold);

				let concerns = Vec::new();

				if results_score == 0 {
					let msg = format!("{:.2}% pull requests without review <= {:.2}% pull requests without review", percent_flagged * 100.0, percent_threshold * 100.0);
					// PANIC: percent_flagged and percent_threshold will never be NaN
					Ok(Rc::new(AnalysisReport::Review {
						value: F64::new(percent_flagged)
							.expect("Percent flagged should never be NaN"),
						threshold: F64::new(percent_threshold)
							.expect("Percent threshold should never be NaN"),
						outcome: AnalysisOutcome::Pass(msg),
						concerns,
					}))
				} else {
					let msg = format!(
						"{:.2}% pull requests without review > {:.2}% pull requests without review",
						percent_flagged * 100.0,
						percent_threshold * 100.0
					);
					// PANIC: percent_flagged and percent_threshold will never be NaN
					Ok(Rc::new(AnalysisReport::Review {
						value: F64::new(percent_flagged)
							.expect("Percent flagged should never be NaN"),
						threshold: F64::new(percent_threshold)
							.expect("Percent threshold should never be NaN"),
						outcome: AnalysisOutcome::Fail(msg),
						concerns,
					}))
				}
			}
		}
	} else {
		Ok(Rc::new(AnalysisReport::None {
			outcome: AnalysisOutcome::Skipped,
		}))
	}
}

pub fn typo_analysis(db: &dyn AnalysisProvider) -> Result<Rc<AnalysisReport>> {
	if db.typo_active() {
		let results = db.typo_metric();

		match results {
			Err(err) => Ok(Rc::new(AnalysisReport::None {
				outcome: AnalysisOutcome::Error(err),
			})),
			Ok(results) => {
				let num_flagged = results.typos.len() as u64;
				let count_threshold = db.typo_count_threshold();
				let results_score = score_by_threshold(num_flagged, count_threshold);

				let concerns: Vec<_> = results
					.typos
					.iter()
					.map(|typodep| Concern::Typo {
						dependency_name: typodep.dependency.to_string(),
					})
					.collect::<HashSet<_>>()
					.into_iter()
					.collect();

				if results_score == 0 {
					let msg = format!(
						"{} possible typos <= {} possible typos",
						num_flagged, count_threshold
					);
					Ok(Rc::new(AnalysisReport::Typo {
						value: num_flagged,
						threshold: count_threshold,
						outcome: AnalysisOutcome::Pass(msg),
						concerns,
					}))
				} else {
					let msg = format!(
						"{} possible typos > {} possible typos",
						num_flagged, count_threshold
					);
					Ok(Rc::new(AnalysisReport::Typo {
						value: num_flagged,
						threshold: count_threshold,
						outcome: AnalysisOutcome::Pass(msg),
						concerns,
					}))
				}
			}
		}
	} else {
		Ok(Rc::new(AnalysisReport::None {
			outcome: AnalysisOutcome::Skipped,
		}))
	}
}

pub fn pr_affiliation_analysis(db: &dyn AnalysisProvider) -> Result<Rc<AnalysisReport>> {
	if db.pr_affiliation_active() {
		let results = db.pr_affiliation_metric();

		match results {
			Err(err) => Ok(Rc::new(AnalysisReport::None {
				outcome: AnalysisOutcome::Error(err),
			})),
			Ok(results) => {
				let affiliated_iter = results
					.affiliations
					.iter()
					.filter(|a| a.affiliated_type.is_affiliated());

				let value = affiliated_iter.clone().count() as u64;
				let threshold = db.pr_affiliation_count_threshold();
				let results_score = score_by_threshold(value, threshold);

				let mut contributor_freq_map = HashMap::new();

				for affiliation in affiliated_iter {
					let commit_view =
						db.get_pr_contributors_for_commit(Rc::clone(&affiliation.commit))?;

					let contributor = match affiliation.affiliated_type {
						AffiliatedType::Author => String::from(&commit_view.author.name),
						AffiliatedType::Committer => String::from(&commit_view.committer.name),
						AffiliatedType::Neither => String::from("Neither"),
						AffiliatedType::Both => String::from("Both"),
					};

					let count_commits_for = |contributor| {
						db.get_pr_commits_for_contributor(Rc::clone(contributor))
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
					.map(|(contributor, count)| PrConcern::PrAffiliation { contributor, count })
					.collect();

				if results_score == 0 {
					let msg = format!("{} affiliated <= {} affiliated", value, threshold);
					Ok(Rc::new(AnalysisReport::PrAffiliation {
						value,
						threshold,
						outcome: AnalysisOutcome::Pass(msg),
						concerns,
					}))
				} else {
					let msg = format!("{} affiliated > {} affiliated", value, threshold);
					Ok(Rc::new(AnalysisReport::PrAffiliation {
						value,
						threshold,
						outcome: AnalysisOutcome::Fail(msg),
						concerns,
					}))
				}
			}
		}
	} else {
		Ok(Rc::new(AnalysisReport::None {
			outcome: AnalysisOutcome::Skipped,
		}))
	}
}

pub fn pr_contributor_trust_analysis(db: &dyn AnalysisProvider) -> Result<Rc<AnalysisReport>> {
	if db.contributor_trust_active() {
		let results = db.pr_contributor_trust_metric();

		match results {
			Err(err) => Ok(Rc::new(AnalysisReport::None {
				outcome: AnalysisOutcome::Error(err),
			})),
			Ok(results) => {
				let num_flagged = results
					.contributor_counts_in_period
					.iter()
					.filter(|c| c.1.pr_trusted)
					.count() as u64;
				let percent_flagged =
					num_flagged as f64 / results.contributor_counts_in_period.len() as f64;
				let percent_threshold = *db.contributor_trust_percent_threshold();
				let results_score = score_by_threshold_reversed(percent_flagged, percent_threshold);
				//if more trusted than threshold, no strike
				//if fewer trusted than threshold, get a strike
				//can not use score_by_threshold because it is the reverse

				let concerns = results
					.contributor_counts_in_period
					.iter()
					.filter(|c| !c.1.pr_trusted)
					.map(|(contributor, ..)| PrConcern::PrContributorTrust {
						contributor: contributor.to_string(),
					})
					.collect();

				if results_score == 0 {
					let msg = format!(
						"{:.2}% contributor trust threshold <= {:.2}% contributor trust threshold",
						percent_flagged * 100.0,
						percent_threshold * 100.0
					);
					// PANIC: percent_flagged and percent_threshold will never be NaN
					Ok(Rc::new(AnalysisReport::PrContributorTrust {
						value: F64::new(percent_flagged)
							.expect("Percent flagged should never be NaN"),
						threshold: F64::new(percent_threshold)
							.expect("Percent threshold should never be NaN"),
						outcome: AnalysisOutcome::Pass(msg),
						concerns,
					}))
				} else {
					let msg = format!(
						"{:.2}% contributor trust threshold > {:.2}% contributor trust threshold",
						percent_flagged * 100.0,
						percent_threshold * 100.0
					);
					// PANIC: percent_flagged and percent_threshold will never be NaN
					Ok(Rc::new(AnalysisReport::PrContributorTrust {
						value: F64::new(percent_flagged)
							.expect("Percent flagged should never be NaN"),
						threshold: F64::new(percent_threshold)
							.expect("Percent threshold should never be NaN"),
						outcome: AnalysisOutcome::Fail(msg),
						concerns,
					}))
				}
			}
		}
	} else {
		Ok(Rc::new(AnalysisReport::None {
			outcome: AnalysisOutcome::Skipped,
		}))
	}
}

pub fn pr_module_contributors_analysis(db: &dyn AnalysisProvider) -> Result<Rc<AnalysisReport>> {
	if db.pr_module_contributors_active() {
		let results = db.pr_module_contributors_metric();

		match results {
			Err(err) => Ok(Rc::new(AnalysisReport::None {
				outcome: AnalysisOutcome::Error(err),
			})),
			Ok(results) => {
				let contributors_map = &results.contributors_map;

				let total_contributors = contributors_map.keys().len();
				let mut flagged_contributors: u64 = 0;

				for contributed_modules in contributors_map.values() {
					for contributed_module in contributed_modules {
						if contributed_module.new_contributor {
							flagged_contributors += 1;
							break;
						}
					}
				}

				let percent_flagged = flagged_contributors as f64 / total_contributors as f64;
				let percent_threshold = *db.pr_module_contributors_percent_threshold();
				let results_score = score_by_threshold(percent_flagged, percent_threshold);

				let concerns = Vec::new();

				if results_score == 0 {
					let msg = format!(
						"{:.2}% contributors contributing a module for the first time <= {:.2}% permitted amount",
						percent_flagged * 100.0,
						percent_threshold * 100.0);
					Ok(Rc::new(AnalysisReport::PrModuleContributors {
						value: F64::new(percent_flagged)
							.expect("Percent flagged should never be NaN"),
						threshold: F64::new(percent_threshold)
							.expect("Percent threshold should never be NaN"),
						outcome: AnalysisOutcome::Pass(msg),
						concerns,
					}))
				} else {
					let msg = format!(
						"{:.2}% contributors contributing a module for the first time >= {:.2}% permitted amount",
						percent_flagged * 100.0,
						percent_threshold * 100.0);
					Ok(Rc::new(AnalysisReport::PrModuleContributors {
						value: F64::new(percent_flagged)
							.expect("Percent flagged should never be NaN"),
						threshold: F64::new(percent_threshold)
							.expect("Percent threshold should never be NaN"),
						outcome: AnalysisOutcome::Fail(msg),
						concerns,
					}))
				}
			}
		}
	} else {
		Ok(Rc::new(AnalysisReport::None {
			outcome: AnalysisOutcome::Skipped,
		}))
	}
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
