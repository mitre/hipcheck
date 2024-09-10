// SPDX-License-Identifier: Apache-2.0

pub use crate::report::*;
use crate::{
	analysis::score::*,
	config::{ConfigSource, RiskConfigQuery},
	engine::HcEngine,
	error::{Error, Result},
	hc_error,
	plugin::{PluginName, PluginPublisher},
	policy::policy_file::PolicyPluginName,
	session::Session,
	source::SourceQuery,
	version::VersionQuery,
};
use serde_json::Value;
use std::{collections::HashSet, default::Default, result::Result as StdResult};

/// Print the final report of a Hipcheck run.
pub fn build_report(session: &Session, scoring: &ScoringResults) -> Result<Report> {
	log::debug!("building final report");

	// This function needs to:
	//
	// 1. Build a report from the information available.
	// 2. Print that report.

	let mut builder = ReportBuilder::for_session(session);

	// Activity analysis.
	if let Some(stored) = &scoring.results.get_legacy(ACTIVITY_PHASE) {
		match &stored.response {
			Ok(res) => {
				let weeks = {
					let Value::Number(opt_weeks) = &res.value else {
						return Err(hc_error!("activity analysis has a non-numeric value"));
					};
					opt_weeks
						.as_u64()
						.ok_or(hc_error!("activity analysis has a non-u64 value"))?
				};
				let analysis = Analysis::activity(weeks, stored.policy.clone(), stored.passed);
				builder.add_analysis(analysis, res.concerns.clone())?;
			}
			Err(error) => {
				builder.add_errored_analysis(AnalysisIdent::Activity, error);
			}
		}
	};

	// Affiliation analysis.
	if let Some(stored) = &scoring.results.get_legacy(AFFILIATION_PHASE) {
		match &stored.response {
			Ok(res) => {
				let length = {
					let Value::Array(arr) = &res.value else {
						return Err(hc_error!("affiliation analysis has a non-array value"));
					};
					arr.len() as u64
				};
				let analysis = Analysis::affiliation(length, stored.policy.clone(), stored.passed);
				if stored.passed {
					builder.add_passing_analysis(analysis);
				} else {
					builder.add_failing_analysis(analysis, res.concerns.clone())?;
				}
			}
			Err(error) => {
				builder.add_errored_analysis(AnalysisIdent::Affiliation, error);
			}
		}
	};

	// Binary analysis
	if let Some(stored) = &scoring.results.get_legacy(BINARY_PHASE) {
		match &stored.response {
			Ok(res) => {
				let length = {
					let Value::Array(arr) = &res.value else {
						return Err(hc_error!("binary analysis has a non-array value"));
					};
					arr.len() as u64
				};
				let analysis = Analysis::binary(length, stored.policy.clone(), stored.passed);
				if stored.passed {
					builder.add_passing_analysis(analysis);
				} else {
					builder.add_failing_analysis(analysis, res.concerns.clone())?;
				}
			}
			Err(error) => {
				builder.add_errored_analysis(AnalysisIdent::Binary, error);
			}
		}
	};

	// Churn analysis.
	if let Some(stored) = &scoring.results.get_legacy(CHURN_PHASE) {
		match &stored.response {
			Ok(res) => {
				let length = {
					let Value::Array(arr) = &res.value else {
						return Err(hc_error!("churn analysis has a non-array value"));
					};
					arr.len() as u64
				};
				let analysis = Analysis::churn(length, stored.policy.clone(), stored.passed);
				if stored.passed {
					builder.add_passing_analysis(analysis);
				} else {
					builder.add_failing_analysis(analysis, res.concerns.clone())?;
				}
			}
			Err(error) => {
				builder.add_errored_analysis(AnalysisIdent::Churn, error);
			}
		}
	};

	// Entropy analysis.
	if let Some(stored) = &scoring.results.get_legacy(ENTROPY_PHASE) {
		match &stored.response {
			Ok(res) => {
				let length = {
					let Value::Array(arr) = &res.value else {
						return Err(hc_error!("entropy analysis has a non-array value"));
					};
					arr.len() as u64
				};
				let analysis = Analysis::entropy(length, stored.policy.clone(), stored.passed);
				if stored.passed {
					builder.add_passing_analysis(analysis);
				} else {
					builder.add_failing_analysis(analysis, res.concerns.clone())?;
				}
			}
			Err(error) => {
				builder.add_errored_analysis(AnalysisIdent::Entropy, error);
			}
		}
	};

	// Identity analysis.
	if let Some(stored) = &scoring.results.get_legacy(IDENTITY_PHASE) {
		match &stored.response {
			Ok(res) => {
				let pct_identity = {
					let Value::Number(opt_identity) = &res.value else {
						return Err(hc_error!("identity analysis has a non-numeric value"));
					};
					opt_identity
						.as_f64()
						.ok_or(hc_error!("activity analysis has a non-float value"))?
				};
				let analysis =
					Analysis::identity(pct_identity, stored.policy.clone(), stored.passed);
				if stored.passed {
					builder.add_passing_analysis(analysis);
				} else {
					builder.add_failing_analysis(analysis, res.concerns.clone())?;
				}
			}
			Err(error) => {
				builder.add_errored_analysis(AnalysisIdent::Identity, error);
			}
		}
	};

	// Fuzz analysis.
	if let Some(stored) = &scoring.results.get_legacy(FUZZ_PHASE) {
		match &stored.response {
			Ok(res) => {
				let Value::Bool(doing_fuzz) = &res.value else {
					return Err(hc_error!("identity analysis has a non-numeric value"));
				};
				let analysis = Analysis::fuzz(*doing_fuzz, stored.policy.clone(), stored.passed);
				if stored.passed {
					builder.add_passing_analysis(analysis);
				} else {
					builder.add_failing_analysis(analysis, res.concerns.clone())?;
				}
			}
			Err(error) => {
				builder.add_errored_analysis(AnalysisIdent::Fuzz, error);
			}
		}
	};

	// Review analysis.
	if let Some(stored) = &scoring.results.get_legacy(REVIEW_PHASE) {
		match &stored.response {
			Ok(res) => {
				let pct_review = {
					let Value::Number(opt_review) = &res.value else {
						return Err(hc_error!("review analysis has a non-numeric value"));
					};
					opt_review
						.as_f64()
						.ok_or(hc_error!("review analysis has a non-float value"))?
				};
				let analysis = Analysis::review(pct_review, stored.policy.clone(), stored.passed);
				if stored.passed {
					builder.add_passing_analysis(analysis);
				} else {
					builder.add_failing_analysis(analysis, res.concerns.clone())?;
				}
			}
			Err(error) => {
				builder.add_errored_analysis(AnalysisIdent::Review, error);
			}
		}
	};

	// Typo analysis.
	if let Some(stored) = &scoring.results.get_legacy(TYPO_PHASE) {
		match &stored.response {
			Ok(res) => {
				let deps = {
					let Value::Array(opt_deps) = &res.value else {
						return Err(hc_error!("typo analysis has a non-numeric value"));
					};
					opt_deps.len() as u64
				};
				let analysis = Analysis::typo(deps, stored.policy.clone(), stored.passed);
				builder.add_analysis(analysis, res.concerns.clone())?;
			}
			Err(error) => {
				builder.add_errored_analysis(AnalysisIdent::Typo, error);
			}
		}
	};

	for (analysis, stored) in scoring.results.plugin_results() {
		let name = format!(
			"{}/{}",
			analysis.publisher.as_str(),
			analysis.plugin.as_str()
		);

		match &stored.response {
			Ok(res) => {
				// This is the "explanation" pulled from the new gRPC call.
				let message = session
					.default_query_explanation(analysis.publisher.clone(), analysis.plugin.clone())?
					.unwrap_or("no query explanation provided".to_owned());

				builder.add_analysis(
					Analysis::plugin(name, stored.passed, stored.policy.clone(), message),
					res.concerns.clone(),
				)?;
			}
			Err(error) => {
				builder.add_errored_analysis(AnalysisIdent::Plugin(name), error);
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

#[allow(unused)]
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
				let (publisher, name) = match &failed.analysis {
					Analysis::Activity { .. } => ("mitre", "activity"),
					Analysis::Affiliation { .. } => ("mitre", "affiliation"),
					Analysis::Binary { .. } => ("mitre", "binary"),
					Analysis::Churn { .. } => ("mitre", "churn"),
					Analysis::Entropy { .. } => ("mitre", "entropy"),
					Analysis::Identity { .. } => ("mitre", "identity"),
					Analysis::Fuzz { .. } => ("mitre", "fuzz"),
					Analysis::Review { .. } => ("mitre", "review"),
					Analysis::Typo { .. } => ("mitre", "typo"),
					Analysis::Plugin { name, .. } => name.as_str().split_once('/').unwrap(),
				};
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
