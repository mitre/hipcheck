// SPDX-License-Identifier: Apache-2.0

pub use crate::report::*;
use crate::{
	analysis::score::*,
	config::{ConfigSource, RiskConfigQuery},
	error::{Error, Result},
	hc_error,
	plugin::{PluginName, PluginPublisher},
	policy::policy_file::PolicyPluginName,
	session::Session,
	source::SourceQuery,
	version::VersionQuery,
};
use std::{collections::HashSet, default::Default};

/// Print the final report of a Hipcheck run.
pub fn build_report(session: &Session, scoring: &ScoringResults) -> Result<Report> {
	log::debug!("building final report");

	// This function needs to:
	//
	// 1. Build a report from the information available.
	// 2. Print that report.

	let mut builder = ReportBuilder::for_session(session);

	for (analysis, stored) in scoring.results.plugin_results() {
		let name = format!(
			"{}/{}",
			analysis.publisher.as_str(),
			analysis.plugin.as_str()
		);

		match &stored.response {
			Ok(res) => {
				builder.add_analysis(
					Analysis::plugin(name, stored.passed, stored.policy.clone()),
					res.concerns.clone(),
				)?;
			}
			Err(error) => {
				builder.add_errored_analysis(AnalysisIdent(name), error);
			}
		}
	}

	// TODO: Add handling of auto-investigation-if-fail rules.

	builder
		.set_risk_score(scoring.score.total)
		.set_risk_policy(session.risk_policy().as_ref().clone());

	let report = builder.build()?;

	log::info!("built final report");

	Ok(report)
}

/// Builds a final `Report` of Hipcheck's results.
pub struct ReportBuilder<'sess> {
	/// The `Session`, containing general data from the run.
	session: &'sess Session,

	/// A lookup of which failed analyses warrant an immediate investigation
	investigate_if_failed: HashSet<PolicyPluginName>,

	/// What analyses passed.
	passing: Vec<PassingAnalysis>,

	/// What analyses failed.
	failing: Vec<FailingAnalysis>,

	/// What analyses encountered errors.
	errored: Vec<ErroredAnalysis>,

	/// What risk threshold was configured for the run.
	risk_policy: Option<String>,

	/// What risk score Hipcheck assigned.
	risk_score: Option<f64>,
}

impl<'sess> ReportBuilder<'sess> {
	/// Initiate building a new `Report`.
	pub fn for_session(session: &'sess Session) -> ReportBuilder<'sess> {
		// Get investigate_if_failed hashset from policy
		let policy = session.policy();
		let investigate_if_failed = policy
			.analyze
			.if_fail
			.as_ref()
			.map_or(HashSet::new(), |x| HashSet::from_iter(x.0.iter().cloned()));

		ReportBuilder {
			session,
			investigate_if_failed,
			passing: Default::default(),
			failing: Default::default(),
			errored: Default::default(),
			risk_policy: Default::default(),
			risk_score: Default::default(),
		}
	}

	/// Add an analysis.
	pub fn add_analysis(&mut self, analysis: Analysis, concerns: Vec<String>) -> Result<&mut Self> {
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
		concerns: Vec<String>,
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
	pub fn set_risk_policy(&mut self, risk_policy: String) -> &mut Self {
		self.risk_policy = Some(risk_policy);
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

			let policy = self
				.risk_policy
				.ok_or_else(|| hc_error!("no risk threshold set for report"))
				.map(RiskPolicy)?;

			// Determine recommendation based on score and investigate policy expr
			let mut rec = Recommendation::is(score, policy)?;

			// Override base recommendation if any `investigate-if-fail` analyses failed
			for failed in failing.iter() {
				let (publisher, name) = failed.analysis.name.as_str().split_once('/').unwrap();
				let policy_plugin_name = PolicyPluginName {
					publisher: PluginPublisher(publisher.to_owned()),
					name: PluginName(name.to_owned()),
				};
				if self.investigate_if_failed.contains(&policy_plugin_name) {
					rec.kind = RecommendationKind::Investigate;
					break;
				}
			}

			rec
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
