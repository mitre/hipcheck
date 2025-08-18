// SPDX-License-Identifier: Apache-2.0

// A report encapsulates the results of a run of Hipcheck, specifically containing:
//
// 1. The successes (which analyses passed, with user-friendly explanations of what's good)
// 2. The concerns (which analyses failed, and _why_)
// 3. The recommendation (pass or investigate)

// The report serves double-duty, because it's both the thing used to print user-friendly
// results on the CLI, and the type that's serialized out to JSON for machine-friendly output.

pub mod report_builder;

use crate::{
	cli::Format,
	error::{Context, Error, Result},
	policy_exprs::{self, std_exec, Expr},
};
use chrono::prelude::*;
use schemars::JsonSchema;
use serde::{Serialize, Serializer};
use serde_json::Value;
use std::{
	default::Default,
	fmt,
	fmt::{Display, Formatter},
	iter::Iterator,
	result::Result as StdResult,
	sync::Arc,
};

/// The report output to the user.
#[derive(Debug, Serialize, JsonSchema)]
#[schemars(crate = "schemars")]
pub struct Report {
	/// The name of the repository being analyzed.
	pub repo_name: Arc<String>,

	/// Optional owner/maintainer of the repo
	pub repo_owner: Arc<Option<String>>,

	/// The HEAD commit hash of the repository during analysis.
	pub repo_head: Arc<String>,

	/// The version of Hipcheck used to analyze the repo.
	pub hipcheck_version: Arc<String>,

	/// When the analysis was performed.
	pub analyzed_at: Timestamp,

	/// What analyses passed.
	pub passing: Vec<PassingAnalysis>,

	/// What analyses did _not_ pass, and why.
	pub failing: Vec<FailingAnalysis>,

	/// What analyses errored out, and why.
	pub errored: Vec<ErroredAnalysis>,

	/// The final recommendation to the user.
	pub recommendation: Recommendation,
}

/// An analysis which passed.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(transparent)]
#[schemars(crate = "schemars")]
pub struct PassingAnalysis(
	/// The analysis which passed.
	Analysis,
);

impl PassingAnalysis {
	pub fn new(analysis: Analysis) -> PassingAnalysis {
		PassingAnalysis(analysis)
	}
}

/// An analysis which failed, including potential specific concerns.
#[derive(Debug, Serialize, JsonSchema)]
#[schemars(crate = "schemars")]
pub struct FailingAnalysis {
	/// The analysis.
	#[serde(flatten)]
	analysis: Analysis,

	/// Any concerns the analysis identified.
	#[serde(skip_serializing_if = "no_concerns")]
	concerns: Vec<String>,
}

impl FailingAnalysis {
	/// Construct a new failing analysis, verifying that concerns are appropriate.
	pub fn new(analysis: Analysis, concerns: Vec<String>) -> Result<FailingAnalysis> {
		Ok(FailingAnalysis { analysis, concerns })
	}
}

/// Is the concern list empty?
///
/// This is a helper function for serialization of `FailedAnalysis`.
fn no_concerns(concerns: &[String]) -> bool {
	concerns.is_empty()
}

/// An analysis that did _not_ succeed.
#[derive(Debug, Serialize, JsonSchema)]
#[schemars(crate = "schemars")]
pub struct ErroredAnalysis {
	analysis: AnalysisIdent,
	name: AnalysisIdent,
	error: ErrorReport,
}

impl ErroredAnalysis {
	/// Construct a new `ErroredAnalysis`.
	pub fn new(analysis: AnalysisIdent, name: AnalysisIdent, error: &Error) -> Self {
		ErroredAnalysis {
			analysis,
			name,
			error: ErrorReport::from(error),
		}
	}
}

/// The name of the analyses.
#[derive(Debug, Serialize, JsonSchema)]
#[schemars(crate = "schemars")]
pub struct AnalysisIdent(String);

impl Display for AnalysisIdent {
	fn fmt(&self, f: &mut Formatter) -> fmt::Result {
		write!(f, "{}", self.0)
	}
}

/// A simple, serializable version of `Error`.
#[derive(Debug, Serialize, JsonSchema)]
#[schemars(crate = "schemars")]
pub struct ErrorReport {
	msg: String,
	#[serde(skip_serializing_if = "source_is_none")]
	source: Option<Box<ErrorReport>>,
}

fn source_is_none(source: &Option<Box<ErrorReport>>) -> bool {
	source.is_none()
}

impl From<&Error> for ErrorReport {
	fn from(error: &Error) -> ErrorReport {
		log::trace!("detailed error for report [error: {:#?}]", error);

		let mut errors = error
			.chain()
			// This `collect` is needed because `hc_error::Chain` isn't a
			// double-ended iterator, so it can't be reversed without
			// first collecting into an intermediate container.
			.collect::<Vec<_>>()
			.into_iter()
			.rev();

		let mut report = ErrorReport {
			// SAFETY: We're always guaranteed a minimum of one error
			// message, so this is safe.
			msg: errors.next().unwrap().to_string(),
			source: None,
		};

		for error in errors {
			report = ErrorReport {
				msg: error.to_string(),
				source: Some(Box::new(report)),
			};
		}

		report
	}
}

impl From<&(dyn std::error::Error + 'static)> for ErrorReport {
	fn from(error: &(dyn std::error::Error + 'static)) -> ErrorReport {
		let msg = error.to_string();
		let source = error
			.source()
			.map(|error| Box::new(ErrorReport::from(error)));
		ErrorReport { msg, source }
	}
}

/// An analysis, with score and threshold.
#[derive(Debug, Serialize, JsonSchema, Clone)]
#[serde(tag = "analysis")]
#[schemars(crate = "schemars")]
pub struct Analysis {
	/// The name of the plugin.
	name: String,

	/// If the analysis is passing or not.
	///
	/// Same as with the message, this is computed eagerly in the case of
	/// plugin analyses.
	passed: bool,

	/// The policy expression used for the plugin.
	///
	/// We use this when printing the result to help explain to the user
	/// *why* an analysis failed.
	#[schemars(schema_with = "String::json_schema")]
	policy_expr: Expr,

	/// The value returned by the analysis, if it exists
	///
	/// This will be set to `None` and not printed unless "debug JSON" format is used
	///
	/// We use this when printing the result to help explain to the user
	/// *why* an analysis failed.
	#[serde(skip_serializing_if = "Option::is_none")]
	value: Option<Value>,

	/// The value returned by the analysis after being computed in the policy expression, if it exists
	///
	/// We also use this when printing the result to help explain to the user
	/// *why* an analysis failed.
	final_value: Option<String>,

	/// Either an English language explanation of why the plugin succeded or failed or the default query explanation (if an English explanation could not be constructed)
	/// The default explanation is pulled from RPC with the plugin.
	message: String,
}

// fn custom_schema(generator: &mut SchemaGenerator) -> Schema {
// 	let mut schema = String::json_schema(generator);
// 	schema
// }

impl Analysis {
	pub fn plugin(
		name: String,
		passed: bool,
		policy_expr: Expr,
		init_value: Option<Value>,
		full_value: bool,
		default_message: String,
	) -> Self {
		// Try to parse the policy expression to an English language explanation of why the plugin's analysis succeded or failed
		// If it cannot be parsed, return the default explanation text
		// At the same time, compute the raw value returned by the plugin in the policy expression and get that value, if it exists
		let (message, final_value) = match policy_exprs::parse_expr_to_english(
			&policy_expr,
			&default_message,
			&init_value,
			passed,
		) {
			Ok(explanation) => explanation,
			Err(e) => {
				log::error!(
					"Could not parse policy expression for {} plugin: {}",
					&name,
					e
				);
				(
					default_message.clone(),
					init_value.as_ref().map(|v| v.to_string()),
				)
			}
		};

		// If not using "debug JSON" format, do not use the raw plugin output values
		let value = match full_value {
			true => init_value,
			false => None,
		};

		Analysis {
			name,
			passed,
			policy_expr,
			value,
			final_value,
			message,
		}
	}

	pub fn is_passing(&self) -> bool {
		self.passed
	}
}

#[allow(unused)]
/// Value and threshold for counting-based analyses.
#[derive(Debug, Serialize, JsonSchema, Clone)]
#[schemars(crate = "schemars")]
pub struct Count {
	value: u64,
	policy: String,
}

#[allow(unused)]
/// Value for binary-based analyses.
#[derive(Debug, Serialize, JsonSchema, Clone)]
#[schemars(crate = "schemars")]
pub struct Exists {
	value: bool,
	policy: String,
}

#[allow(unused)]
/// Value and threshold for percentage-based analyses.
#[derive(Debug, Serialize, JsonSchema, Clone)]
#[schemars(crate = "schemars")]
pub struct Percent {
	value: f64,
	policy: String,
}

/// A final recommendation of whether to use or investigate a piece of software,
/// including the risk threshold associated with that decision.
#[derive(Debug, Serialize, JsonSchema, Clone)]
#[schemars(crate = "schemars")]
pub struct Recommendation {
	pub kind: RecommendationKind,
	pub reason: Option<InvestigateReason>,
	risk_score: RiskScore,
	risk_policy: RiskPolicy,
}

impl Recommendation {
	/// Make a recommendation.
	pub fn is(risk_score: RiskScore, risk_policy: RiskPolicy) -> Result<Recommendation> {
		let kind = RecommendationKind::is(risk_score, risk_policy.clone())?;
		let reason = match kind {
			RecommendationKind::Pass => None,
			RecommendationKind::Investigate => Some(InvestigateReason::Policy),
		};

		Ok(Recommendation {
			kind,
			reason,
			risk_score,
			risk_policy,
		})
	}
}

/// The kind of recommendation being made.
#[derive(Debug, Serialize, JsonSchema, Clone, Copy)]
#[schemars(crate = "schemars")]
pub enum RecommendationKind {
	Pass,
	Investigate,
}

/// Describe why the the recommendation is to investigate
#[derive(Debug, Serialize, JsonSchema, Clone)]
#[schemars(crate = "schemars")]
pub enum InvestigateReason {
	Policy,
	FailedAnalyses(Vec<String>),
}

impl RecommendationKind {
	fn is(risk_score: RiskScore, risk_policy: RiskPolicy) -> Result<RecommendationKind> {
		let value = serde_json::to_value(risk_score.0).unwrap();
		Ok(
			if std_exec(risk_policy.expr.clone(), Some(&value))
				.context("investigate policy expression execution failed")?
			{
				RecommendationKind::Pass
			} else {
				RecommendationKind::Investigate
			},
		)
	}
}

/// The overall final risk score for a repo.
#[derive(Debug, Serialize, JsonSchema, Clone, Copy)]
#[serde(transparent)]
#[schemars(crate = "schemars")]
pub struct RiskScore(pub f64);

/// The risk threshold configured for the Hipcheck session.
#[derive(Debug, Serialize, JsonSchema, Clone)]
#[serde(transparent)]
#[schemars(crate = "schemars")]
pub struct RiskPolicy {
	#[schemars(schema_with = "String::json_schema")]
	pub expr: Expr,
}
impl RiskPolicy {
	pub fn new(expr: Expr) -> Self {
		RiskPolicy { expr }
	}
}

/// A serializable and printable wrapper around a datetime with the local timezone.
#[derive(Debug, JsonSchema)]
#[schemars(crate = "schemars")]
pub struct Timestamp(DateTime<Local>);

impl From<DateTime<FixedOffset>> for Timestamp {
	fn from(date_time: DateTime<FixedOffset>) -> Timestamp {
		Timestamp(date_time.with_timezone(&Local))
	}
}

impl Display for Timestamp {
	fn fmt(&self, f: &mut Formatter) -> fmt::Result {
		// This is more human-readable than RFC 3339, which is good since this method
		// will be used when outputting to end-users on the CLI.
		write!(f, "{}", self.0.format("%a %B %-d, %Y at %-I:%M%P"))
	}
}

impl Serialize for Timestamp {
	fn serialize<S>(&self, serializer: S) -> StdResult<S::Ok, S::Error>
	where
		S: Serializer,
	{
		// The format is "1996-12-19T16:39:57-08:00"
		//
		// This isn't very human readable, but in the case of human output we won't be
		// serializing anyway, so that's fine. The point here is to be machine-readable
		// and use minimal space.
		serializer.serialize_str(&self.0.to_rfc3339())
	}
}
