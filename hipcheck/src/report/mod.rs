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
	hash::{Hash, Hasher},
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
	concerns: Vec<Concern>,
}

impl FailingAnalysis {
	/// Construct a new failing analysis, verifying that concerns are appropriate.
	pub fn new(analysis: Analysis, concerns: Vec<Concern>) -> Result<FailingAnalysis> {
		match analysis {
			Analysis::Activity { .. } | Analysis::Affiliation { .. } => {
				if concerns.iter().all(Concern::is_affiliation_concern).not() {
					return Err(hc_error!(
						"affiliation analysis results include non-affiliation concerns"
					));
				}
			}
			Analysis::Fuzz { .. } | Analysis::Identity { .. } | Analysis::Review { .. } => {
				if concerns.is_empty().not() {
					return Err(hc_error!(
						"{} analysis doesn't support attaching concerns",
						analysis.name()
					));
				}
			}
			Analysis::Binary { .. } => {
				if concerns.iter().all(Concern::is_binary_file_concern).not() {
					return Err(hc_error!(
						"binary file analysis results include non-binary file concerns"
					));
				}
			}
			Analysis::Churn { .. } => {
				if concerns.iter().all(Concern::is_churn_concern).not() {
					return Err(hc_error!(
						"churn analysis results include non-churn concerns"
					));
				}
			}
			Analysis::Entropy { .. } => {
				if concerns.iter().all(Concern::is_entropy_concern).not() {
					return Err(hc_error!(
						"entropy analysis results include non-entropy concerns"
					));
				}
			}
			Analysis::Typo { .. } => {
				if concerns.iter().all(Concern::is_typo_concern).not() {
					return Err(hc_error!("typo analysis results include non-typo concerns"));
				}
			}
			Analysis::Plugin { .. } => {
				if concerns.iter().all(Concern::is_plugin_concern).not() {
					return Err(hc_error!(
						"plugin analysis results include non-plugin concerns",
					));
				}
			}
		}

		Ok(FailingAnalysis { analysis, concerns })
	}

	pub fn analysis(&self) -> &Analysis {
		&self.analysis
	}

	pub fn concerns(&self) -> impl Iterator<Item = &Concern> {
		self.concerns.iter()
	}
}

/// Is the concern list empty?
///
/// This is a helper function for serialization of `FailedAnalysis`.
fn no_concerns(concerns: &[Concern]) -> bool {
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
	},
	/// Affiliation analysis.
	Affiliation {
		#[serde(flatten)]
		scoring: Count,
	},
	/// Binary file analysis
	Binary {
		#[serde(flatten)]
		scoring: Count,
	},
	/// Churn analysis.
	Churn {
		#[serde(flatten)]
		scoring: Percent,
	},
	/// Entropy analysis.
	Entropy {
		#[serde(flatten)]
		scoring: Percent,
	},
	/// Identity analysis.
	Identity {
		#[serde(flatten)]
		scoring: Percent,
	},
	/// Fuzz repo analysis
	Fuzz {
		#[serde(flatten)]
		scoring: Exists,
	},
	/// Review analysis.
	Review {
		#[serde(flatten)]
		scoring: Percent,
	},
	/// Typo analysis.
	Typo {
		#[serde(flatten)]
		scoring: Count,
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
			pub fn $name(value: $type, threshold: $type) -> Analysis {
				Analysis::[<$name:camel>] { scoring: $container { value, threshold }}
			}
		}
	};
}

macro_rules! exists_constructor_paste {
	( $name:tt($type:ty), $container:ident ) => {
		paste! {
			pub fn $name(value: $type) -> Analysis {
				Analysis::[<$name:camel>] { scoring: $container { value }}
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
	percent_constructor!(churn);
	percent_constructor!(entropy);
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
			Fuzz {
				scoring: Exists { value },
			} => *value,
			Activity {
				scoring: Count { value, threshold },
			}
			| Affiliation {
				scoring: Count { value, threshold },
			}
			| Binary {
				scoring: Count { value, threshold },
			}
			| Typo {
				scoring: Count { value, threshold },
			} => value <= threshold,
			Churn {
				scoring: Percent { value, threshold },
			}
			| Entropy {
				scoring: Percent { value, threshold },
			}
			| Identity {
				scoring: Percent { value, threshold },
			}
			| Review {
				scoring: Percent { value, threshold },
			} => value <= threshold,
			Plugin { is_passing, .. } => *is_passing,
		}
	}

	/// Indicates if the analysis will print concerns.
	///
	/// Currently, we suppress concerns if an analysis passes,
	/// and this is the method that implements that suppression.
	pub fn permits_some_concerns(&self) -> bool {
		use Analysis::*;

		match self {
			Fuzz { scoring: _ } => true,
			Activity {
				scoring: Count { threshold, .. },
			}
			| Affiliation {
				scoring: Count { threshold, .. },
			}
			| Binary {
				scoring: Count { threshold, .. },
			}
			| Typo {
				scoring: Count { threshold, .. },
			} => *threshold != 0,
			Churn {
				scoring: Percent { threshold, .. },
			}
			| Entropy {
				scoring: Percent { threshold, .. },
			}
			| Identity {
				scoring: Percent { threshold, .. },
			}
			| Review {
				scoring: Percent { threshold, .. },
			} => *threshold != 0.0,
			// Don't suppress concerns for plugins.
			Plugin { .. } => true,
		}
	}

	pub fn statement(&self) -> String {
		use Analysis::*;

		match self {
			Activity { .. } => {
				if self.is_passing() {
					"has been updated recently".to_string()
				} else {
					"hasn't been updated recently".to_string()
				}
			}
			Affiliation { .. } => match (self.is_passing(), self.permits_some_concerns()) {
				(true, true) => "few concerning contributors".to_string(),
				(true, false) => "no concerning contributors".to_string(),
				(false, true) => "too many concerning contributors".to_string(),
				(false, false) => "has concerning contributors".to_string(),
			},
			Binary { .. } => match (self.is_passing(), self.permits_some_concerns()) {
				(true, true) => "few concerning binary files".to_string(),
				(true, false) => "no concerning binary files".to_string(),
				(false, true) => "too many concerning binary files".to_string(),
				(false, false) => "has concerning binary files".to_string(),
			},
			Churn { .. } => match (self.is_passing(), self.permits_some_concerns()) {
				(true, true) => "few unusually large commits".to_string(),
				(true, false) => "no unusually large commits".to_string(),
				(false, true) => "too many unusually large commits".to_string(),
				(false, false) => "has unusually large commits".to_string(),
			},
			Entropy { .. } => match (self.is_passing(), self.permits_some_concerns()) {
				(true, true) => "few unusual-looking commits".to_string(),
				(true, false) => "no unusual-looking commits".to_string(),
				(false, true) => "too many unusual-looking commits".to_string(),
				(false, false) => "has unusual-looking commits".to_string(),
			},
			Identity { .. } => {
				if self.is_passing() {
					"commits often applied by person besides the author".to_string()
				} else {
					"commits too often applied by the author".to_string()
				}
			}
			Fuzz { .. } => {
				if self.is_passing() {
					"repository receives regular fuzz testing".to_string()
				} else {
					"repository does not receive regular fuzz testing".to_string()
				}
			}
			Review { .. } => {
				if self.is_passing() {
					"change requests often receive approving review prior to merge".to_string()
				} else {
					"change requests often lack approving review prior to merge".to_string()
				}
			}
			Typo { .. } => match (self.is_passing(), self.permits_some_concerns()) {
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
				scoring: Count { value, threshold },
			} => format!(
				"updated {} weeks ago, required in the last {} weeks",
				value, threshold
			),
			Affiliation {
				scoring: Count { value, threshold },
			} => format!("{} found, {} permitted", value, threshold),
			Binary {
				scoring: Count { value, threshold },
			} => format!("{} found, {} permitted", value, threshold),
			Churn {
				scoring: Percent { value, threshold },
			} => format!(
				"{:.2}% of commits are unusually large, {:.2}% permitted",
				value * 100.0,
				threshold * 100.0
			),
			Entropy {
				scoring: Percent { value, threshold },
			} => format!(
				"{:.2}% of commits are unusual-looking, {:.2}% permitted",
				value * 100.0,
				threshold * 100.0
			),
			Identity {
				scoring: Percent { value, threshold },
			} => format!(
				"{:.2}% of commits merged by author, {:.2}% permitted",
				value * 100.0,
				threshold * 100.0
			),
			Fuzz {
				scoring: Exists { value, .. },
			} => format!("fuzzing integration found: {}", value),
			Review {
				scoring: Percent { value, threshold },
			} => format!(
				"{:.2}% did not receive review, {:.2}% permitted",
				value * 100.0,
				threshold * 100.0,
			),
			Typo {
				scoring: Count { value, threshold },
			} => format!(
				"{} concerning dependencies found, {} permitted",
				value, threshold
			),
			Plugin { message, .. } => message.clone(),
		}
	}
}

/// A specific concern identified by an analysis.
///
/// Note that `Concern` _isn't_ capturing all types of failed analyses, it's
/// only used where additional detail / evidence should be provided beyond the
/// top-level information.
#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(untagged)]
#[schemars(crate = "schemars")]
#[allow(unused)]
pub enum Concern {
	/// Commits with affiliated contributor(s)
	Affiliation { contributor: String, count: i64 },

	/// Suspect binary files
	Binary { file_path: String },

	/// Commits with high churn.
	Churn {
		commit_hash: String,
		score: f64,
		threshold: f64,
	},
	/// Commits with high entropy.
	Entropy {
		commit_hash: String,
		score: f64,
		threshold: f64,
	},
	/// Commits with typos.
	Typo { dependency_name: String },
	/// A concern arising from a plugin.
	///
	/// These concerns are always unstructured, as the plugin is expected to
	/// handle the creation of the concern string in a way that is friendly to
	/// the end-user.
	Plugin(String),
}

impl Concern {
	/// Check if a concern is an affiliation concern.
	fn is_affiliation_concern(&self) -> bool {
		matches!(self, Concern::Affiliation { .. })
	}

	/// Check if a concern is a binary file concern.
	fn is_binary_file_concern(&self) -> bool {
		matches!(self, Concern::Binary { .. })
	}

	/// Check if a concern is a churn concern.
	fn is_churn_concern(&self) -> bool {
		matches!(self, Concern::Churn { .. })
	}

	/// Check if a concern is an entropy concern.
	fn is_entropy_concern(&self) -> bool {
		matches!(self, Concern::Entropy { .. })
	}

	/// Check if a concern is a typo concern.
	fn is_typo_concern(&self) -> bool {
		matches!(self, Concern::Typo { .. })
	}

	#[allow(unused)]
	/// Check if the concern is from a plugin.
	fn is_plugin_concern(&self) -> bool {
		matches!(self, Concern::Plugin(..))
	}

	pub fn description(&self) -> String {
		use Concern::*;

		match self {
			Affiliation { contributor, count } => {
				format!("{} - affiliation count {}", contributor, count)
			}
			Binary { file_path } => {
				format!("binary file: {}", file_path)
			}
			Churn {
				commit_hash,
				score,
				threshold,
			} => format!(
				"{} - churn score {}, expected {} or lower",
				commit_hash, score, threshold
			),
			Entropy {
				commit_hash,
				score,
				threshold,
			} => format!(
				"{} - entropy score {:.2}, expected {} or lower",
				commit_hash, score, threshold
			),
			Typo { dependency_name } => format!("Dependency '{}' may be a typo", dependency_name),
			Plugin(msg) => msg.clone(),
		}
	}
}

impl Hash for Concern {
	fn hash<H: Hasher>(&self, state: &mut H) {
		match self {
			Concern::Affiliation { contributor, .. } => contributor.hash(state),
			Concern::Binary { file_path, .. } => file_path.hash(state),
			Concern::Churn { commit_hash, .. } => commit_hash.hash(state),
			Concern::Entropy { commit_hash, .. } => commit_hash.hash(state),
			Concern::Typo { dependency_name } => dependency_name.hash(state),
			Concern::Plugin(msg) => msg.hash(state),
		}
	}
}

impl PartialEq for Concern {
	fn eq(&self, other: &Concern) -> bool {
		match (self, other) {
			(
				Concern::Binary { file_path },
				Concern::Binary {
					file_path: other_file_path,
				},
			) => file_path == other_file_path,
			(
				Concern::Churn { commit_hash, .. },
				Concern::Churn {
					commit_hash: other_commit_hash,
					..
				},
			) => commit_hash == other_commit_hash,
			(
				Concern::Entropy { commit_hash, .. },
				Concern::Entropy {
					commit_hash: other_commit_hash,
					..
				},
			) => commit_hash == other_commit_hash,
			(
				Concern::Typo { dependency_name },
				Concern::Typo {
					dependency_name: other_dependency_name,
				},
			) => dependency_name == other_dependency_name,
			(Concern::Plugin(msg), Concern::Plugin(other_msg)) => msg == other_msg,
			_ => false,
		}
	}
}

impl Eq for Concern {}

/// The result of running an analysis.
pub enum AnalysisResult<T, E> {
	Ran(T),
	Error(E),
	Skip,
}

impl<'opt, T, E> From<&'opt Option<StdResult<T, E>>> for AnalysisResult<&'opt T, &'opt E> {
	fn from(other: &'opt Option<StdResult<T, E>>) -> AnalysisResult<&'opt T, &'opt E> {
		match other {
			Some(Ok(t)) => AnalysisResult::Ran(t),
			Some(Err(e)) => AnalysisResult::Error(e),
			None => AnalysisResult::Skip,
		}
	}
}

/// Value and threshold for counting-based analyses.
#[derive(Debug, Serialize, JsonSchema, Clone, Copy)]
#[schemars(crate = "schemars")]
pub struct Count {
	value: u64,
	threshold: u64,
}

/// Value for binary-based analyses.
#[derive(Debug, Serialize, JsonSchema, Clone, Copy)]
#[schemars(crate = "schemars")]
pub struct Exists {
	value: bool,
}

/// Value and threshold for percentage-based analyses.
#[derive(Debug, Serialize, JsonSchema, Clone, Copy)]
#[schemars(crate = "schemars")]
pub struct Percent {
	value: f64,
	threshold: f64,
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
