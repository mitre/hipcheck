// SPDX-License-Identifier: Apache-2.0

use crate::{analysis::AnalysisReport, score::ScoringResults, session::Session};
use hc_common::{
	config::RiskConfigQuery,
	error::{Error, Result},
	hc_error, log,
};
use hc_data::source::SourceQuery;
pub use hc_report::*;
use std::default::Default;
use std::result::Result as StdResult;

/// Print the final report of a Hipcheck run.
pub fn build_report(session: &Session, scoring: &ScoringResults) -> Result<Report> {
	log::debug!("building final report");

	// This function needs to:
	//
	// 1. Build a report from the information available.
	// 2. Print that report.

	let mut builder = ReportBuilder::for_session(session);

	// Activity analysis.
	extract_results(
		&mut builder,
		&scoring.results.activity,
		|builder, output| {
			match output.as_ref() {
				AnalysisReport::Activity {
					value,
					threshold,
					concerns,
					..
				} => {
					let analysis = Analysis::activity(*value, *threshold);
					builder.add_analysis(analysis, concerns.clone())?;
				}
				_ => {
					return Err(hc_error!(
						"phase name does not match {} analysis",
						crate::score::ACTIVITY_PHASE
					))
				}
			}

			Ok(())
		},
		|builder, error| {
			builder.add_errored_analysis(AnalysisIdent::Activity, error);
			Ok(())
		},
	)?;

	// Affiliation analysis.

	extract_results(
		&mut builder,
		&scoring.results.affiliation,
		|builder, output| {
			match output.as_ref() {
				AnalysisReport::Affiliation {
					value,
					threshold,
					concerns,
					..
				} => {
					let analysis = Analysis::affiliation(*value, *threshold);
					builder.add_analysis(analysis, concerns.clone())?;
				}
				_ => {
					return Err(hc_error!(
						"phase name does not match {} analysis",
						crate::score::AFFILIATION_PHASE
					))
				}
			}

			Ok(())
		},
		|builder, error| {
			builder.add_errored_analysis(AnalysisIdent::Affiliation, error);
			Ok(())
		},
	)?;

	// Binary Analysis

	extract_results(
		&mut builder,
		&scoring.results.binary,
		|builder, output| {
			match output.as_ref() {
				AnalysisReport::Binary {
					value,
					threshold,
					concerns,
					..
				} => {
					let analysis = Analysis::binary(*value, *threshold);
					builder.add_analysis(analysis, concerns.clone())?;
				}
				_ => {
					return Err(hc_error!(
						"phase name does not match {} analysis",
						crate::score::BINARY_PHASE
					))
				}
			}

			Ok(())
		},
		|builder, error| {
			builder.add_errored_analysis(AnalysisIdent::Binary, error);
			Ok(())
		},
	)?;

	// Churn analysis.
	extract_results(
		&mut builder,
		&scoring.results.churn,
		|builder, output| {
			match output.as_ref() {
				AnalysisReport::Churn {
					value,
					threshold,
					concerns,
					..
				} => {
					let analysis = Analysis::churn(value.into_inner(), threshold.into_inner());
					builder.add_analysis(analysis, concerns.clone())?;
				}
				_ => {
					return Err(hc_error!(
						"phase name does not match {} analysis",
						crate::score::CHURN_PHASE
					))
				}
			}

			Ok(())
		},
		|builder, error| {
			builder.add_errored_analysis(AnalysisIdent::Churn, error);
			Ok(())
		},
	)?;

	// Entropy analysis.
	extract_results(
		&mut builder,
		&scoring.results.entropy,
		|builder, output| {
			match output.as_ref() {
				AnalysisReport::Entropy {
					value,
					threshold,
					concerns,
					..
				} => {
					let analysis = Analysis::entropy(value.into_inner(), threshold.into_inner());
					builder.add_analysis(analysis, concerns.clone())?;
				}
				_ => {
					return Err(hc_error!(
						"phase name does not match {} analysis",
						crate::score::ENTROPY_PHASE
					))
				}
			}

			Ok(())
		},
		|builder, error| {
			builder.add_errored_analysis(AnalysisIdent::Entropy, error);
			Ok(())
		},
	)?;

	// Identity analysis.
	extract_results(
		&mut builder,
		&scoring.results.identity,
		|builder, output| {
			match output.as_ref() {
				AnalysisReport::Identity {
					value,
					threshold,
					concerns,
					..
				} => {
					let analysis = Analysis::identity(value.into_inner(), threshold.into_inner());
					builder.add_analysis(analysis, concerns.clone())?;
				}
				_ => {
					return Err(hc_error!(
						"phase name does not match {} analysis",
						crate::score::IDENTITY_PHASE
					))
				}
			}

			Ok(())
		},
		|builder, error| {
			builder.add_errored_analysis(AnalysisIdent::Identity, error);
			Ok(())
		},
	)?;

	// Fuzz analysis.
	extract_results(
		&mut builder,
		&scoring.results.fuzz,
		|builder, output| {
			match output.as_ref() {
				AnalysisReport::Fuzz {
					value, concerns, ..
				} => {
					let analysis = Analysis::fuzz(*value);
					builder.add_analysis(analysis, concerns.clone())?;
				}
				_ => {
					return Err(hc_error!(
						"phase name does not match {} analysis",
						crate::score::FUZZ_PHASE
					))
				}
			}

			Ok(())
		},
		|builder, error| {
			builder.add_errored_analysis(AnalysisIdent::Fuzz, error);
			Ok(())
		},
	)?;

	// Review analysis.
	extract_results(
		&mut builder,
		&scoring.results.review,
		|builder, output| {
			match output.as_ref() {
				AnalysisReport::Review {
					value,
					threshold,
					concerns,
					..
				} => {
					let analysis = Analysis::review(value.into_inner(), threshold.into_inner());
					builder.add_analysis(analysis, concerns.clone())?;
				}
				_ => {
					return Err(hc_error!(
						"phase name does not match {} analysis",
						crate::score::REVIEW_PHASE
					))
				}
			}

			Ok(())
		},
		|builder, error| {
			builder.add_errored_analysis(AnalysisIdent::Review, error);
			Ok(())
		},
	)?;

	// Typo analysis.
	extract_results(
		&mut builder,
		&scoring.results.typo,
		|builder, output| {
			match output.as_ref() {
				AnalysisReport::Typo {
					value,
					threshold,
					concerns,
					..
				} => {
					let analysis = Analysis::typo(*value, *threshold);
					builder.add_analysis(analysis, concerns.clone())?;
				}
				_ => {
					return Err(hc_error!(
						"phase name does not match {} analysis",
						crate::score::TYPO_PHASE
					))
				}
			}

			Ok(())
		},
		|builder, error| {
			builder.add_errored_analysis(AnalysisIdent::Typo, error);
			Ok(())
		},
	)?;

	builder
		.set_risk_score(scoring.score.total)
		.set_risk_threshold(*session.risk_threshold());

	let report = builder.build()?;

	log::info!("built final report");

	Ok(report)
}

fn extract_results<O, E, P, F>(
	builder: &mut ReportBuilder,
	result: &Option<StdResult<O, E>>,
	pass: P,
	fail: F,
) -> Result<()>
where
	P: Fn(&mut ReportBuilder, &O) -> Result<()>,
	F: Fn(&mut ReportBuilder, &E) -> Result<()>,
{
	match AnalysisResult::from(result) {
		// Handle successes.
		AnalysisResult::Ran(output) => pass(builder, output),
		// Handle errors.
		AnalysisResult::Error(error) => fail(builder, error),
		// Do nothing if skipped.
		AnalysisResult::Skip => Ok(()),
	}
}

/// Builds a final `Report` of Hipcheck's results.
pub struct ReportBuilder<'sess> {
	/// The `Session`, containing general data from the run.
	session: &'sess Session,

	/// What analyses passed.
	passing: Vec<PassingAnalysis>,

	/// What analyses failed.
	failing: Vec<FailingAnalysis>,

	/// What analyses encountered errors.
	errored: Vec<ErroredAnalysis>,

	/// What risk threshold was configured for the run.
	risk_threshold: Option<f64>,

	/// What risk score Hipcheck assigned.
	risk_score: Option<f64>,
}

impl<'sess> ReportBuilder<'sess> {
	/// Initiate building a new `Report`.
	pub fn for_session(session: &'sess Session) -> ReportBuilder<'sess> {
		ReportBuilder {
			session,
			passing: Default::default(),
			failing: Default::default(),
			errored: Default::default(),
			risk_threshold: Default::default(),
			risk_score: Default::default(),
		}
	}

	/// Add an analysis.
	pub fn add_analysis(
		&mut self,
		analysis: Analysis,
		concerns: Vec<Concern>,
	) -> Result<&mut Self> {
		if analysis.is_passing() {
			Ok(self.add_passing_analysis(analysis))
		} else {
			self.add_failing_analysis(analysis, concerns)
		}
	}

	/// Add an errored analysis to the report.
	pub fn add_errored_analysis(&mut self, analysis: AnalysisIdent, error: &Error) -> &mut Self {
		self.errored.push(ErroredAnalysis::new(analysis, error));
		self
	}

	/// Add an analysis that passed.
	fn add_passing_analysis(&mut self, analysis: Analysis) -> &mut Self {
		self.passing.push(PassingAnalysis::new(analysis));
		self
	}

	/// Add a failing analysis and any concerns associated with it.
	fn add_failing_analysis(
		&mut self,
		analysis: Analysis,
		concerns: Vec<Concern>,
	) -> Result<&mut Self> {
		self.failing.push(FailingAnalysis::new(analysis, concerns)?);
		Ok(self)
	}

	/// Set the overall risk score for the report.
	pub fn set_risk_score(&mut self, risk_score: f64) -> &mut Self {
		self.risk_score = Some(risk_score);
		self
	}

	/// Set what's being recommended to the user.
	pub fn set_risk_threshold(&mut self, risk_threshold: f64) -> &mut Self {
		self.risk_threshold = Some(risk_threshold);
		self
	}

	/// Build a new report.
	///
	/// The `recommendation_kind` and `risk_score` _must_ be set before calling `build`,
	/// or building will fail.
	pub fn build(self) -> Result<Report> {
		let repo_name = self.session.name();
		let repo_head = self.session.head();
		let hipcheck_version = self.session.hc_version().to_string();
		let analyzed_at = Timestamp::from(self.session.started_at());
		let passing = self.passing;
		let failing = self.failing;
		let errored = self.errored;
		let recommendation = {
			let score = self
				.risk_score
				.ok_or_else(|| hc_error!("no risk score set for report"))
				.map(RiskScore)?;

			let threshold = self
				.risk_threshold
				.ok_or_else(|| hc_error!("no risk threshold set for report"))
				.map(RiskThreshold)?;

			Recommendation::is(score, threshold)
		};

		let report = Report {
			repo_name,
			repo_head,
			hipcheck_version,
			analyzed_at,
			passing,
			failing,
			errored,
			recommendation,
		};

		Ok(report)
	}
}

/// Print the final report of a Hipcheck pull request run.
pub fn build_pr_report(session: &Session, scoring: &ScoringResults) -> Result<PrReport> {
	log::debug!("building final report for pull request");

	// This function needs to:
	//
	// 1. Build a report from the information available.
	// 2. Print that report.

	let mut builder = PrReportBuilder::for_session(session);

	// Pull request affiliation analysis.
	extract_pr_results(
		&mut builder,
		&scoring.results.pr_affiliation,
		|builder, output| {
			match output.as_ref() {
				AnalysisReport::PrAffiliation {
					value,
					threshold,
					concerns,
					..
				} => {
					let analysis = PrAnalysis::pr_affiliation(*value, *threshold);
					builder.add_analysis(analysis, concerns.clone())?;
				}
				_ => {
					return Err(hc_error!(
						"phase name does not match {} analysis",
						crate::score::PR_AFFILIATION_PHASE
					))
				}
			}

			Ok(())
		},
		|builder, error| {
			builder.add_errored_analysis(PrAnalysisIdent::PrAffiliation, error);
			Ok(())
		},
	)?;

	// Pull request contributor analysis.
	extract_pr_results(
		&mut builder,
		&scoring.results.pr_contributor_trust,
		|builder, output| {
			match output.as_ref() {
				AnalysisReport::PrContributorTrust {
					value,
					threshold,
					concerns,
					..
				} => {
					let analysis = PrAnalysis::pr_contributor_trust(
						value.into_inner(),
						threshold.into_inner(),
					);
					builder.add_analysis(analysis, concerns.clone())?;
				}
				_ => {
					return Err(hc_error!(
						"phase name does not match {} analysis",
						crate::score::PR_CONTRIBUTOR_TRUST_PHASE
					))
				}
			}

			Ok(())
		},
		|builder, error| {
			builder.add_errored_analysis(PrAnalysisIdent::PrContributorTrust, error);
			Ok(())
		},
	)?;

	// Pull request module contributors analysis.
	extract_pr_results(
		&mut builder,
		&scoring.results.pr_module_contributors,
		|builder, output| {
			match output.as_ref() {
				AnalysisReport::PrModuleContributors {
					value,
					threshold,
					concerns,
					..
				} => {
					let analysis = PrAnalysis::pr_module_contributors(
						value.into_inner(),
						threshold.into_inner(),
					);
					builder.add_analysis(analysis, concerns.clone())?;
				}
				_ => {
					return Err(hc_error!(
						"phase name does not match {} analysis",
						crate::score::PR_MODULE_CONTRIBUTORS_PHASE
					))
				}
			}

			Ok(())
		},
		|builder, error| {
			builder.add_errored_analysis(PrAnalysisIdent::PrModuleContributors, error);
			Ok(())
		},
	)?;

	builder
		.set_risk_score(scoring.score.total)
		.set_risk_threshold(*session.risk_threshold());

	let report = builder.build()?;

	log::info!("built final report for pull request");

	Ok(report)
}

fn extract_pr_results<O, E, P, F>(
	builder: &mut PrReportBuilder,
	result: &Option<StdResult<O, E>>,
	pass: P,
	fail: F,
) -> Result<()>
where
	P: Fn(&mut PrReportBuilder, &O) -> Result<()>,
	F: Fn(&mut PrReportBuilder, &E) -> Result<()>,
{
	match AnalysisResult::from(result) {
		// Handle successes.
		AnalysisResult::Ran(output) => pass(builder, output),
		// Handle errors.
		AnalysisResult::Error(error) => fail(builder, error),
		// Do nothing if skipped.
		AnalysisResult::Skip => Ok(()),
	}
}

/// Builds a final `Report` of Hipcheck's results.
pub struct PrReportBuilder<'sess> {
	/// The `Session`, containing general data from the run.
	session: &'sess Session,

	/// What analyses passed.
	passing: Vec<PrPassingAnalysis>,

	/// What analyses failed.
	failing: Vec<PrFailingAnalysis>,

	/// What analyses encountered errors.
	errored: Vec<PrErroredAnalysis>,

	/// What risk threshold was configured for the run.
	risk_threshold: Option<f64>,

	/// What risk score Hipcheck assigned.
	risk_score: Option<f64>,
}

impl<'sess> PrReportBuilder<'sess> {
	/// Initiate building a new `Report`.
	pub fn for_session(session: &'sess Session) -> PrReportBuilder<'sess> {
		PrReportBuilder {
			session,
			passing: Default::default(),
			failing: Default::default(),
			errored: Default::default(),
			risk_threshold: Default::default(),
			risk_score: Default::default(),
		}
	}

	/// Add an analysis.
	pub fn add_analysis(
		&mut self,
		analysis: PrAnalysis,
		concerns: Vec<PrConcern>,
	) -> Result<&mut Self> {
		if analysis.is_passing() {
			Ok(self.add_passing_analysis(analysis))
		} else {
			self.add_failing_analysis(analysis, concerns)
		}
	}

	/// Add an errored analysis to the report.
	pub fn add_errored_analysis(&mut self, analysis: PrAnalysisIdent, error: &Error) -> &mut Self {
		self.errored.push(PrErroredAnalysis::new(analysis, error));
		self
	}

	/// Add an analysis that passed.
	fn add_passing_analysis(&mut self, analysis: PrAnalysis) -> &mut Self {
		self.passing.push(PrPassingAnalysis::new(analysis));
		self
	}

	/// Add a failing analysis and any concerns associated with it.
	fn add_failing_analysis(
		&mut self,
		analysis: PrAnalysis,
		concerns: Vec<PrConcern>,
	) -> Result<&mut Self> {
		self.failing
			.push(PrFailingAnalysis::new(analysis, concerns)?);
		Ok(self)
	}

	/// Set the overall risk score for the report.
	pub fn set_risk_score(&mut self, risk_score: f64) -> &mut Self {
		self.risk_score = Some(risk_score);
		self
	}

	/// Set what's being recommended to the user.
	pub fn set_risk_threshold(&mut self, risk_threshold: f64) -> &mut Self {
		self.risk_threshold = Some(risk_threshold);
		self
	}

	/// Build a new report.
	///
	/// The `recommendation_kind` and `risk_score` _must_ be set before calling `build`,
	/// or building will fail.
	pub fn build(self) -> Result<PrReport> {
		let pr_uri = self
			.session
			.url()
			.ok_or_else(|| hc_error!("not a valid report"))?;
		let repo_head = self.session.head();
		let hipcheck_version = self.session.hc_version().to_string();
		let analyzed_at = Timestamp::from(self.session.started_at());
		let passing = self.passing;
		let failing = self.failing;
		let errored = self.errored;
		let recommendation = {
			let score = self
				.risk_score
				.ok_or_else(|| hc_error!("no risk score set for report"))
				.map(RiskScore)?;

			let threshold = self
				.risk_threshold
				.ok_or_else(|| hc_error!("no risk threshold set for report"))
				.map(RiskThreshold)?;

			Recommendation::is(score, threshold)
		};

		let report = PrReport {
			pr_uri,
			repo_head,
			hipcheck_version,
			analyzed_at,
			passing,
			failing,
			errored,
			recommendation,
		};

		Ok(report)
	}
}
