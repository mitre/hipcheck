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
	hc_error,
	policy_exprs::Executor,
	version::VersionQuery,
};
use chrono::prelude::*;
use paste::paste;
use schemars::JsonSchema;
use serde::{Serialize, Serializer};
use std::{
	default::Default,
	fmt,
	fmt::{Display, Formatter},
	iter::Iterator,
	ops::Not as _,
	result::Result as StdResult,
	sync::Arc,
};

/// The report output to the user.
#[derive(Debug, Serialize, JsonSchema)]
#[schemars(crate = "schemars")]
pub struct Report {
	/// The name of the repository being analyzed.
	pub repo_name: Arc<String>,

	/// The HEAD commit hash of the repository during analysis.
	pub repo_head: Arc<String>,

	/// The version of Hipcheck used to analyze the repo.
	pub hipcheck_version: String,

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

impl Report {
	/// Get the repository that was analyzed.
	pub fn analyzed(&self) -> String {
		format!("{} ({})", self.repo_name, self.repo_head)
	}

	/// Get the version of Hipcheck used for the analysis.
	pub fn using(&self) -> String {
		format!("using Hipcheck {}", self.hipcheck_version)
	}

	// Get the time that the analysis occured.
	pub fn at_time(&self) -> String {
		format!("on {}", self.analyzed_at)
	}

	/// Check if there are passing analyses.
	pub fn has_passing_analyses(&self) -> bool {
		self.passing.is_empty().not()
	}

	/// Check if there are failing analyses.
	pub fn has_failing_analyses(&self) -> bool {
		self.failing.is_empty().not()
	}

	/// Check if there are errored analyses.
	pub fn has_errored_analyses(&self) -> bool {
		self.errored.is_empty().not()
	}

	/// Get an iterator over all passing analyses.
	pub fn passing_analyses(&self) -> impl Iterator<Item = &Analysis> {
		self.passing.iter().map(|a| &a.0)
	}

	/// Get an iterator over all failing analyses.
	pub fn failing_analyses(&self) -> impl Iterator<Item = &FailingAnalysis> {
		self.failing.iter()
	}

	/// Get an iterator over all errored analyses.
	pub fn errored_analyses(&self) -> impl Iterator<Item = &ErroredAnalysis> {
		self.errored.iter()
	}

	/// Get the final recommendation.
	pub fn recommendation(&self) -> &Recommendation {
		&self.recommendation
	}
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
		match analysis {
			Analysis::Fuzz { .. } | Analysis::Identity { .. } | Analysis::Review { .. } => {
				if concerns.is_empty().not() {
					return Err(hc_error!(
						"{} analysis doesn't support attaching concerns",
						analysis.name()
					));
				}
			}
			_ => (),
		};

		Ok(FailingAnalysis { analysis, concerns })
	}

	pub fn analysis(&self) -> &Analysis {
		&self.analysis
	}

	pub fn concerns(&self) -> impl Iterator<Item = &String> {
		self.concerns.iter()
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
	error: ErrorReport,
}

impl ErroredAnalysis {
	/// Construct a new `ErroredAnalysis`.
	pub fn new(analysis: AnalysisIdent, error: &Error) -> Self {
		ErroredAnalysis {
			analysis,
			error: ErrorReport::from(error),
		}
	}

	pub fn top_msg(&self) -> String {
		format!("{} analysis error: {}", self.analysis, self.error.msg)
	}

	pub fn source_msgs(&self) -> Vec<String> {
		let mut msgs = Vec::new();

		try_add_msg(&mut msgs, &self.error.source);

		msgs
	}
}

fn try_add_msg(msgs: &mut Vec<String>, error_report: &Option<Box<ErrorReport>>) {
	if let Some(error_report) = error_report {
		msgs.push(error_report.msg.clone());
		try_add_msg(msgs, &error_report.source);
	}
}

/// The name of the analyses.
#[derive(Debug, Serialize, JsonSchema)]
#[schemars(crate = "schemars")]
pub enum AnalysisIdent {
	Activity,
	Affiliation,
	Binary,
	Churn,
	Entropy,
	Identity,
	Fuzz,
	Review,
	Typo,
	Plugin(String),
}

impl Display for AnalysisIdent {
	fn fmt(&self, f: &mut Formatter) -> fmt::Result {
		use AnalysisIdent::*;

		let name = match self {
			Activity => "activity",
			Affiliation => "affiliation",
			Binary => "binary",
			Churn => "churn",
			Entropy => "entropy",
			Identity => "identity",
			Fuzz => "fuzz",
			Review => "review",
			Typo => "typo",
			Plugin(name) => name,
		};

		write!(f, "{}", name)
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
pub enum Analysis {
	/// Activity analysis.
	Activity {
		#[serde(flatten)]
		scoring: Count,
		#[serde(skip)]
		passed: bool,
	},
	/// Affiliation analysis.
	Affiliation {
		#[serde(flatten)]
		scoring: Count,
		#[serde(skip)]
		passed: bool,
	},
	/// Binary file analysis
	Binary {
		#[serde(flatten)]
		scoring: Count,
		#[serde(skip)]
		passed: bool,
	},
	/// Churn analysis.
	Churn {
		#[serde(flatten)]
		scoring: Count,
		#[serde(skip)]
		passed: bool,
	},
	/// Entropy analysis.
	Entropy {
		#[serde(flatten)]
		scoring: Count,
		#[serde(skip)]
		passed: bool,
	},
	/// Identity analysis.
	Identity {
		#[serde(flatten)]
		scoring: Percent,
		#[serde(skip)]
		passed: bool,
	},
	/// Fuzz repo analysis
	Fuzz {
		#[serde(flatten)]
		scoring: Exists,
		#[serde(skip)]
		passed: bool,
	},
	/// Review analysis.
	Review {
		#[serde(flatten)]
		scoring: Percent,
		#[serde(skip)]
		passed: bool,
	},
	/// Typo analysis.
	Typo {
		#[serde(flatten)]
		scoring: Count,
		#[serde(skip)]
		passed: bool,
	},
	#[allow(unused)]
	/// Plugin analysis.
	Plugin {
		/// The name of the plugin.
		name: String,
		/// If the analysis is passing or not.
		///
		/// Same as with the message, this is computed eagerly in the case of
		/// plugin analyses.
		is_passing: bool,
		/// The policy expression used for the plugin.
		///
		/// We use this when printing the result to help explain to the user
		/// *why* an analysis failed.
		policy_expr: String,
		/// The default query explanation pulled from RPC with the plugin.
		message: String,
	},
}

macro_rules! constructor {
	( $name:tt($type:ty), $container:ident ) => {
		paste! {
			pub fn $name(value: $type, policy: String, passed: bool) -> Analysis {
				Analysis::[<$name:camel>] { scoring: $container { value, policy }, passed }
			}
		}
	};
}

macro_rules! exists_constructor_paste {
	( $name:tt($type:ty), $container:ident ) => {
		paste! {
			pub fn $name(value: $type, policy: String, passed: bool) -> Analysis {
				Analysis::[<$name:camel>] { scoring: $container { value, policy }, passed }
			}
		}
	};
}

macro_rules! exists_constructor {
	( $name:tt ) => {
		exists_constructor_paste!($name(bool), Exists);
	};
}

macro_rules! count_constructor {
	( $name:tt ) => {
		constructor!($name(u64), Count);
	};
}

macro_rules! percent_constructor {
	( $name:tt ) => {
		constructor!($name(f64), Percent);
	};
}

impl Analysis {
	exists_constructor!(fuzz);
	count_constructor!(activity);
	count_constructor!(affiliation);
	count_constructor!(binary);
	count_constructor!(typo);
	count_constructor!(churn);
	count_constructor!(entropy);
	percent_constructor!(identity);
	percent_constructor!(review);

	pub fn plugin(name: String, is_passing: bool, policy_expr: String, message: String) -> Self {
		Analysis::Plugin {
			name,
			is_passing,
			policy_expr,
			message,
		}
	}

	/// Get the name of the analysis, for printing.
	pub fn name(&self) -> &str {
		match self {
			Analysis::Activity { .. } => "activity",
			Analysis::Affiliation { .. } => "affiliation",
			Analysis::Binary { .. } => "binary",
			Analysis::Churn { .. } => "churn",
			Analysis::Entropy { .. } => "entropy",
			Analysis::Identity { .. } => "identity",
			Analysis::Review { .. } => "review",
			Analysis::Fuzz { .. } => "fuzz",
			Analysis::Typo { .. } => "typo",
			Analysis::Plugin { name, .. } => name,
		}
	}

	pub fn is_passing(&self) -> bool {
		use Analysis::*;

		match self {
			Fuzz { passed, .. }
			| Activity { passed, .. }
			| Affiliation { passed, .. }
			| Binary { passed, .. }
			| Typo { passed, .. }
			| Churn { passed, .. }
			| Entropy { passed, .. }
			| Identity { passed, .. }
			| Review { passed, .. }
			| Plugin {
				is_passing: passed, ..
			} => *passed,
		}
	}

	/// Indicates if the analysis will print concerns.
	///
	/// Currently, we suppress concerns if an analysis passes,
	/// and this is the method that implements that suppression.
	pub fn permits_some_concerns(&self) -> bool {
		true
	}

	pub fn statement(&self) -> String {
		use Analysis::*;

		match self {
			Activity { passed, .. } => {
				if *passed {
					"has been updated recently".to_string()
				} else {
					"hasn't been updated recently".to_string()
				}
			}
			Affiliation { passed, .. } => match (*passed, self.permits_some_concerns()) {
				(true, true) => "few concerning contributors".to_string(),
				(true, false) => "no concerning contributors".to_string(),
				(false, true) => "too many concerning contributors".to_string(),
				(false, false) => "has concerning contributors".to_string(),
			},
			Binary { passed, .. } => match (*passed, self.permits_some_concerns()) {
				(true, true) => "few concerning binary files".to_string(),
				(true, false) => "no concerning binary files".to_string(),
				(false, true) => "too many concerning binary files".to_string(),
				(false, false) => "has concerning binary files".to_string(),
			},
			Churn { passed, .. } => match (*passed, self.permits_some_concerns()) {
				(true, true) => "few unusually large commits".to_string(),
				(true, false) => "no unusually large commits".to_string(),
				(false, true) => "too many unusually large commits".to_string(),
				(false, false) => "has unusually large commits".to_string(),
			},
			Entropy { passed, .. } => match (*passed, self.permits_some_concerns()) {
				(true, true) => "few unusual-looking commits".to_string(),
				(true, false) => "no unusual-looking commits".to_string(),
				(false, true) => "too many unusual-looking commits".to_string(),
				(false, false) => "has unusual-looking commits".to_string(),
			},
			Identity { passed, .. } => {
				if *passed {
					"commits often applied by person besides the author".to_string()
				} else {
					"commits too often applied by the author".to_string()
				}
			}
			Fuzz { passed, .. } => {
				if *passed {
					"repository receives regular fuzz testing".to_string()
				} else {
					"repository does not receive regular fuzz testing".to_string()
				}
			}
			Review { passed, .. } => {
				if *passed {
					"change requests often receive approving review prior to merge".to_string()
				} else {
					"change requests often lack approving review prior to merge".to_string()
				}
			}
			Typo { passed, .. } => match (passed, self.permits_some_concerns()) {
				(true, true) => "few concerning dependency names".to_string(),
				(true, false) => "no concerning dependency names".to_string(),
				(false, true) => "too many concerning dependency names".to_string(),
				(false, false) => "has concerning dependency names".to_string(),
			},
			Plugin { policy_expr, .. } => format!("failed to meet policy: ({policy_expr})"),
		}
	}

	pub fn explanation(&self) -> String {
		use Analysis::*;

		match self {
			Activity {
				scoring: Count { value, policy },
				..
			} => format!("updated {} weeks ago, policy was {}", value, policy),
			Affiliation {
				scoring: Count { value, policy },
				..
			} => format!("{} found, policy was {}", value, policy),
			Binary {
				scoring: Count { value, policy },
				..
			} => format!("{} found, policy was {}", value, policy),
			Churn {
				scoring: Count { value, policy },
				..
			} => format!("examined {} commits, policy was {}", value, policy),
			Entropy {
				scoring: Count { value, policy },
				..
			} => format!("examined {} commits, policy was {}", value, policy),
			Identity {
				scoring: Percent { value, policy },
				..
			} => format!(
				"{:.2}% of commits merged by author, policy was {}",
				value * 100.0,
				policy
			),
			Fuzz {
				scoring: Exists { value, policy },
				..
			} => format!(
				"fuzzing integration found: {}, policy was {}",
				value, policy
			),
			Review {
				scoring: Percent { value, policy },
				..
			} => format!(
				"{:.2}% did not receive review, policy was {}",
				value * 100.0,
				policy
			),
			Typo {
				scoring: Count { value, policy },
				..
			} => format!(
				"{} concerning dependencies found, policy was {}",
				value, policy
			),
			Plugin { message, .. } => message.clone(),
		}
	}
}

/// Value and threshold for counting-based analyses.
#[derive(Debug, Serialize, JsonSchema, Clone)]
#[schemars(crate = "schemars")]
pub struct Count {
	value: u64,
	policy: String,
}

/// Value for binary-based analyses.
#[derive(Debug, Serialize, JsonSchema, Clone)]
#[schemars(crate = "schemars")]
pub struct Exists {
	value: bool,
	policy: String,
}

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
	risk_score: RiskScore,
	risk_policy: RiskPolicy,
}

impl Recommendation {
	/// Make a recommendation.
	pub fn is(risk_score: RiskScore, risk_policy: RiskPolicy) -> Result<Recommendation> {
		let kind = RecommendationKind::is(risk_score, risk_policy.clone())?;

		Ok(Recommendation {
			kind,
			risk_score,
			risk_policy,
		})
	}

	pub fn statement(&self) -> String {
		format!(
			"risk rated as {:.2}, policy was {}",
			self.risk_score.0, self.risk_policy.0
		)
	}
}

/// The kind of recommendation being made.
#[derive(Debug, Serialize, JsonSchema, Clone, Copy)]
#[schemars(crate = "schemars")]
pub enum RecommendationKind {
	Pass,
	Investigate,
}

impl RecommendationKind {
	fn is(risk_score: RiskScore, risk_policy: RiskPolicy) -> Result<RecommendationKind> {
		let value = serde_json::to_value(risk_score.0).unwrap();
		Ok(
			if Executor::std()
				.run(&risk_policy.0, &value)
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
pub struct RiskPolicy(pub String);

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

/// Queries for how Hipcheck reports session results
#[salsa::query_group(ReportParamsStorage)]
pub trait ReportParams: VersionQuery {
	/// Returns the time the current Hipcheck session started
	#[salsa::input]
	fn started_at(&self) -> DateTime<FixedOffset>;

	/// Returns the format of the final report
	#[salsa::input]
	fn format(&self) -> Format;
}
