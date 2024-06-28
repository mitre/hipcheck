// SPDX-License-Identifier: Apache-2.0

use crate::analysis::result::{HCBasicValue, HCPredicate, Predicate};
use crate::analysis::score::*;
use crate::config::RiskConfigQuery;
use crate::error::Error;
use crate::error::Result;
use crate::hc_error;
use crate::report::Concern;
pub use crate::report::*;
use crate::session::session::Session;
use crate::source::source::SourceQuery;
use crate::version::VersionQuery;
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
	if let Some(stored) = &scoring.results.table.get(ACTIVITY_PHASE) {
		match &stored.result {
			Ok(analysis) => {
				let Predicate::Threshold(pred) = analysis.as_ref();
				let HCBasicValue::Unsigned(value) = pred.value else {
					return Err(hc_error!("activity analysis has a non-u64 value"));
				};
				let HCBasicValue::Unsigned(thresh) = pred.threshold else {
					return Err(hc_error!("activity analysis has a non-u64 value"));
				};
				builder.add_analysis(Analysis::activity(value, thresh), stored.concerns.clone())?;
			}
			Err(error) => {
				builder.add_errored_analysis(AnalysisIdent::Activity, error);
			}
		}
	};

	// Affiliation analysis.
	if let Some(stored) = &scoring.results.table.get(AFFILIATION_PHASE) {
		match &stored.result {
			Ok(analysis) => {
				let Predicate::Threshold(pred) = analysis.as_ref();
				let HCBasicValue::Unsigned(value) = pred.value else {
					return Err(hc_error!("affiliation analysis has a non-u64 value"));
				};
				let HCBasicValue::Unsigned(thresh) = pred.threshold else {
					return Err(hc_error!("affiliation analysis has a non-u64 value"));
				};
				builder.add_analysis(
					Analysis::affiliation(value, thresh),
					stored.concerns.clone(),
				)?;
			}
			Err(error) => {
				builder.add_errored_analysis(AnalysisIdent::Affiliation, error);
			}
		}
	};

	// Binary analysis
	if let Some(stored) = &scoring.results.table.get(BINARY_PHASE) {
		match &stored.result {
			Ok(analysis) => {
				let Predicate::Threshold(pred) = analysis.as_ref();
				let HCBasicValue::Unsigned(value) = pred.value else {
					return Err(hc_error!("binary analysis has a non-u64 value"));
				};
				let HCBasicValue::Unsigned(thresh) = pred.threshold else {
					return Err(hc_error!("binary analysis has a non-u64 value"));
				};
				builder.add_analysis(Analysis::binary(value, thresh), stored.concerns.clone())?;
			}
			Err(error) => {
				builder.add_errored_analysis(AnalysisIdent::Binary, error);
			}
		}
	};

	// Churn analysis.
	if let Some(stored) = &scoring.results.table.get(CHURN_PHASE) {
		match &stored.result {
			Ok(analysis) => {
				let Predicate::Threshold(pred) = analysis.as_ref();
				let HCBasicValue::Float(value) = pred.value else {
					return Err(hc_error!("churn analysis has a non-f64 value"));
				};
				let HCBasicValue::Float(thresh) = pred.threshold else {
					return Err(hc_error!("churn analysis has a non-f64 value"));
				};
				builder.add_analysis(
					Analysis::churn(value.into(), thresh.into()),
					stored.concerns.clone(),
				)?;
			}
			Err(error) => {
				builder.add_errored_analysis(AnalysisIdent::Churn, error);
			}
		}
	};

	// Entropy analysis.
	if let Some(stored) = &scoring.results.table.get(ENTROPY_PHASE) {
		match &stored.result {
			Ok(analysis) => {
				let Predicate::Threshold(pred) = analysis.as_ref();
				let HCBasicValue::Float(value) = pred.value else {
					return Err(hc_error!("entropy analysis has a non-f64 value"));
				};
				let HCBasicValue::Float(thresh) = pred.threshold else {
					return Err(hc_error!("entropy analysis has a non-f64 value"));
				};
				builder.add_analysis(
					Analysis::entropy(value.into(), thresh.into()),
					stored.concerns.clone(),
				)?;
			}
			Err(error) => {
				builder.add_errored_analysis(AnalysisIdent::Entropy, error);
			}
		}
	};

	// Identity analysis.
	if let Some(stored) = &scoring.results.table.get(IDENTITY_PHASE) {
		match &stored.result {
			Ok(analysis) => {
				let Predicate::Threshold(pred) = analysis.as_ref();
				let HCBasicValue::Float(value) = pred.value else {
					return Err(hc_error!("identity analysis has a non-f64 value"));
				};
				let HCBasicValue::Float(thresh) = pred.threshold else {
					return Err(hc_error!("identity analysis has a non-f64 value"));
				};
				builder.add_analysis(
					Analysis::identity(value.into(), thresh.into()),
					stored.concerns.clone(),
				)?;
			}
			Err(error) => {
				builder.add_errored_analysis(AnalysisIdent::Identity, error);
			}
		}
	};

	// Fuzz analysis.
	if let Some(stored) = &scoring.results.table.get(FUZZ_PHASE) {
		match &stored.result {
			Ok(analysis) => {
				let Predicate::Threshold(pred) = analysis.as_ref();
				builder.add_analysis(Analysis::fuzz(pred.pass()?), stored.concerns.clone())?;
			}
			Err(error) => {
				builder.add_errored_analysis(AnalysisIdent::Fuzz, error);
			}
		}
	};

	// Review analysis.
	if let Some(stored) = &scoring.results.table.get(REVIEW_PHASE) {
		match &stored.result {
			Ok(analysis) => {
				let Predicate::Threshold(pred) = analysis.as_ref();
				let HCBasicValue::Float(value) = pred.value else {
					return Err(hc_error!("review analysis has a non-f64 value"));
				};
				let HCBasicValue::Float(thresh) = pred.threshold else {
					return Err(hc_error!("review analysis has a non-f64 value"));
				};
				builder.add_analysis(
					Analysis::review(value.into(), thresh.into()),
					stored.concerns.clone(),
				)?;
			}
			Err(error) => {
				builder.add_errored_analysis(AnalysisIdent::Review, error);
			}
		}
	};

	// Typo analysis.
	if let Some(stored) = &scoring.results.table.get(TYPO_PHASE) {
		match &stored.result {
			Ok(analysis) => {
				let Predicate::Threshold(pred) = analysis.as_ref();
				let HCBasicValue::Unsigned(value) = pred.value else {
					return Err(hc_error!("typo analysis has a non-u64 value"));
				};
				let HCBasicValue::Unsigned(thresh) = pred.threshold else {
					return Err(hc_error!("typo analysis has a non-u64 value"));
				};
				builder.add_analysis(Analysis::typo(value, thresh), stored.concerns.clone())?;
			}
			Err(error) => {
				builder.add_errored_analysis(AnalysisIdent::Typo, error);
			}
		}
	};

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
