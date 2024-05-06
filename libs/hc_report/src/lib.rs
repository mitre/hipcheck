// SPDX-License-Identifier: Apache-2.0

// A report encapsulates the results of a run of Hipcheck, specifically containing:
//
// 1. The successes (which analyses passed, with user-friendly explanations of what's good)
// 2. The concerns (which analyses failed, and _why_)
// 3. The recommendation (pass or investigate)

// The report serves double-duty, because it's both the thing used to print user-friendly
// results on the CLI, and the type that's serialized out to JSON for machine-friendly output.

mod query;

pub use query::*;

use hc_common::{
	chrono::prelude::*,
	error::{Error, Result},
	hc_error, log,
	schemars::{self, JsonSchema},
};
use paste::paste;
use serde::{Serialize, Serializer};
use std::default::Default;
use std::fmt::{self, Display, Formatter};
use std::hash::{Hash, Hasher};
use std::iter::Iterator;
use std::ops::Not as _;
use std::rc::Rc;
use std::result::Result as StdResult;

/// The format to report results in.
#[derive(Debug, Clone, Copy)]
pub enum Format {
	/// JSON format.
	Json,
	/// Human-readable format.
	Human,
}

impl Format {
	/// Set if the format is JSON.
	pub fn use_json(json: bool) -> Format {
		if json {
			Format::Json
		} else {
			Format::Human
		}
	}
}

#[derive(Debug)]
pub enum AnyReport {
	Report(Report),
	PrReport(PrReport),
}

/// The report output to the user.
#[derive(Debug, Serialize, JsonSchema)]
#[schemars(crate = "self::schemars")]
pub struct Report {
	/// The name of the repository being analyzed.
	pub repo_name: Rc<String>,

	/// The HEAD commit hash of the repository during analysis.
	pub repo_head: Rc<String>,

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
	pub fn analyzed(&self) -> String {
		format!("{} ({})", self.repo_name, self.repo_head)
	}

	pub fn using(&self) -> String {
		format!("using Hipcheck {}", self.hipcheck_version)
	}

	pub fn at_time(&self) -> String {
		format!("on {}", self.analyzed_at)
	}

	pub fn has_passing_analyses(&self) -> bool {
		self.passing.is_empty().not()
	}

	pub fn has_failing_analyses(&self) -> bool {
		self.failing.is_empty().not()
	}

	pub fn has_errored_analyses(&self) -> bool {
		self.errored.is_empty().not()
	}

	pub fn passing_analyses(&self) -> impl Iterator<Item = &Analysis> {
		self.passing.iter().map(|a| &a.0)
	}

	pub fn failing_analyses(&self) -> impl Iterator<Item = &FailingAnalysis> {
		self.failing.iter()
	}

	pub fn errored_analyses(&self) -> impl Iterator<Item = &ErroredAnalysis> {
		self.errored.iter()
	}

	pub fn recommendation(&self) -> &Recommendation {
		&self.recommendation
	}
}

/// An analysis which passed.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(transparent)]
#[schemars(crate = "self::schemars")]
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
#[schemars(crate = "self::schemars")]
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
#[schemars(crate = "self::schemars")]
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
#[schemars(crate = "self::schemars")]
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
		};

		write!(f, "{}", name)
	}
}

/// A simple, serializable version of `Error`.
#[derive(Debug, Serialize, JsonSchema)]
#[schemars(crate = "self::schemars")]
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
#[derive(Debug, Serialize, JsonSchema, Clone, Copy)]
#[serde(tag = "analysis")]
#[schemars(crate = "self::schemars")]
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

	/// Get the name of the analysis, for printing.
	pub fn name(&self) -> &'static str {
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
		}
	}

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
		}
	}

	pub fn statement(&self) -> String {
		use Analysis::*;

		String::from(match self {
			Activity { .. } => {
				if self.is_passing() {
					"has been updated recently"
				} else {
					"hasn't been updated recently"
				}
			}
			Affiliation { .. } => match (self.is_passing(), self.permits_some_concerns()) {
				(true, true) => "few concerning contributors",
				(true, false) => "no concerning contributors",
				(false, true) => "too many concerning contributors",
				(false, false) => "has concerning contributors",
			},
			Binary { .. } => match (self.is_passing(), self.permits_some_concerns()) {
				(true, true) => "few concerning binary files",
				(true, false) => "no concerning binary files",
				(false, true) => "too many concerning binary files",
				(false, false) => "has concerning binary files",
			},
			Churn { .. } => match (self.is_passing(), self.permits_some_concerns()) {
				(true, true) => "few unusually large commits",
				(true, false) => "no unusually large commits",
				(false, true) => "too many unusually large commits",
				(false, false) => "has unusually large commits",
			},
			Entropy { .. } => match (self.is_passing(), self.permits_some_concerns()) {
				(true, true) => "few unusual-looking commits",
				(true, false) => "no unusual-looking commits",
				(false, true) => "too many unusual-looking commits",
				(false, false) => "has unusual-looking commits",
			},
			Identity { .. } => {
				if self.is_passing() {
					"commits often applied by person besides the author"
				} else {
					"commits too often applied by the author"
				}
			}
			Fuzz { .. } => {
				if self.is_passing() {
					"repository receives regular fuzz testing"
				} else {
					"repository does not receive regular fuzz testing"
				}
			}
			Review { .. } => {
				if self.is_passing() {
					"change requests often receive approving review prior to merge"
				} else {
					"change requests often lack approving review prior to merge"
				}
			}
			Typo { .. } => match (self.is_passing(), self.permits_some_concerns()) {
				(true, true) => "few concerning dependency names",
				(true, false) => "no concerning dependency names",
				(false, true) => "too many concerning dependency names",
				(false, false) => "has concerning dependency names",
			},
		})
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
#[schemars(crate = "self::schemars")]
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

			_ => false,
		}
	}
}

impl Eq for Concern {}

/// The pull request report output to the user.
#[derive(Debug, Serialize, JsonSchema)]
#[schemars(crate = "self::schemars")]
pub struct PrReport {
	/// The URI of the pull request being analyzed.
	pub pr_uri: Rc<String>,

	/// The HEAD commit hash of the repository during analysis.
	pub repo_head: Rc<String>,

	/// The version of Hipcheck used to analyze the repo.
	pub hipcheck_version: String,

	/// When the analysis was performed.
	pub analyzed_at: Timestamp,

	/// What analyses passed.
	pub passing: Vec<PrPassingAnalysis>,

	/// What analyses did _not_ pass, and why.
	pub failing: Vec<PrFailingAnalysis>,

	/// What analyses errored out, and why.
	pub errored: Vec<PrErroredAnalysis>,

	/// The final recommendation to the user.
	pub recommendation: Recommendation,
}

impl PrReport {
	pub fn analyzed(&self) -> String {
		format!("{} ({})", self.pr_uri, self.repo_head)
	}

	pub fn using(&self) -> String {
		format!("using Hipcheck {}", self.hipcheck_version)
	}

	pub fn at_time(&self) -> String {
		format!("on {}", self.analyzed_at)
	}

	pub fn has_passing_analyses(&self) -> bool {
		self.passing.is_empty().not()
	}

	pub fn has_failing_analyses(&self) -> bool {
		self.failing.is_empty().not()
	}

	pub fn has_errored_analyses(&self) -> bool {
		self.errored.is_empty().not()
	}

	pub fn passing_analyses(&self) -> impl Iterator<Item = &PrAnalysis> {
		self.passing.iter().map(|a| &a.0)
	}

	pub fn failing_analyses(&self) -> impl Iterator<Item = &PrFailingAnalysis> {
		self.failing.iter()
	}

	pub fn errored_analyses(&self) -> impl Iterator<Item = &PrErroredAnalysis> {
		self.errored.iter()
	}

	pub fn recommendation(&self) -> &Recommendation {
		&self.recommendation
	}
}

/// An analysis which passed.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(transparent)]
#[schemars(crate = "self::schemars")]
pub struct PrPassingAnalysis(
	/// The analysis which passed.
	PrAnalysis,
);

impl PrPassingAnalysis {
	pub fn new(analysis: PrAnalysis) -> PrPassingAnalysis {
		PrPassingAnalysis(analysis)
	}
}

/// An analysis which failed, including potential specific concerns.
#[derive(Debug, Serialize, JsonSchema)]
#[schemars(crate = "self::schemars")]
pub struct PrFailingAnalysis {
	/// The analysis.
	#[serde(flatten)]
	analysis: PrAnalysis,

	/// Any concerns the analysis identified.
	#[serde(skip_serializing_if = "pr_no_concerns")]
	concerns: Vec<PrConcern>,
}

impl PrFailingAnalysis {
	/// Construct a new failing analysis, verifying that concerns are appropriate.
	pub fn new(analysis: PrAnalysis, concerns: Vec<PrConcern>) -> Result<PrFailingAnalysis> {
		match analysis {
			PrAnalysis::PrAffiliation { .. } => {
				if concerns
					.iter()
					.all(PrConcern::is_pr_affiliation_concern)
					.not()
				{
					return Err(hc_error!("pull request affiliation analysis results include non-pull request affiliation concerns"));
				}
			}
			PrAnalysis::PrContributorTrust { .. } => {
				if concerns
					.iter()
					.all(PrConcern::is_pr_contributor_trust_concern)
					.not()
				{
					return Err(hc_error!("pull request contributor trust analysis results include non-pull request contributor trust concerns"));
				}
			}
			PrAnalysis::PrModuleContributors { .. } => {
				if concerns.is_empty().not() {
					return Err(hc_error!(
						"{} analysis doesn't support attaching concerns",
						analysis.name()
					));
				}
			}
		}
		Ok(PrFailingAnalysis { analysis, concerns })
	}

	pub fn analysis(&self) -> &PrAnalysis {
		&self.analysis
	}

	pub fn concerns(&self) -> impl Iterator<Item = &PrConcern> {
		self.concerns.iter()
	}
}

/// Is the concern list empty?
///
/// This is a helper function for serialization of `FailedAnalysis`.
fn pr_no_concerns(concerns: &[PrConcern]) -> bool {
	concerns.is_empty()
}

/// An analysis that did _not_ succeed.
#[derive(Debug, Serialize, JsonSchema)]
#[schemars(crate = "self::schemars")]
pub struct PrErroredAnalysis {
	analysis: PrAnalysisIdent,
	error: ErrorReport,
}

impl PrErroredAnalysis {
	/// Construct a new `ErroredAnalysis`.
	pub fn new(analysis: PrAnalysisIdent, error: &Error) -> Self {
		PrErroredAnalysis {
			analysis,
			error: ErrorReport::from(error),
		}
	}

	pub fn top_msg(&self) -> String {
		format!("{} analysis error: {}", self.analysis, self.error.msg)
	}

	pub fn source_msgs(&self) -> Vec<String> {
		let mut msgs = Vec::new();

		pr_try_add_msg(&mut msgs, &self.error.source);

		msgs
	}
}

fn pr_try_add_msg(msgs: &mut Vec<String>, error_report: &Option<Box<ErrorReport>>) {
	if let Some(error_report) = error_report {
		msgs.push(error_report.msg.clone());
		try_add_msg(msgs, &error_report.source);
	}
}

/// The name of the analyses.
#[derive(Debug, Serialize, JsonSchema)]
#[schemars(crate = "self::schemars")]
pub enum PrAnalysisIdent {
	PrAffiliation,
	PrContributorTrust,
	PrModuleContributors,
}

impl Display for PrAnalysisIdent {
	fn fmt(&self, f: &mut Formatter) -> fmt::Result {
		use PrAnalysisIdent::*;

		let name = match self {
			PrAffiliation => "single pull request affiliation",
			PrContributorTrust => "pull request contributor trust",
			PrModuleContributors => "pull request module contributors",
		};

		write!(f, "{}", name)
	}
}

/// An analysis, with score and threshold.
#[derive(Debug, Serialize, JsonSchema, Clone, Copy)]
#[serde(tag = "analysis")]
#[schemars(crate = "self::schemars")]
pub enum PrAnalysis {
	/// Pull request affiliation analysis.
	PrAffiliation {
		#[serde(flatten)]
		scoring: Count,
	},
	/// Pull request contributor trust analysis.
	PrContributorTrust {
		#[serde(flatten)]
		scoring: Percent,
	},
	/// Pull request module contributors analysis.
	PrModuleContributors {
		#[serde(flatten)]
		scoring: Percent,
	},
}

macro_rules! pr_constructor {
	( $name:tt($type:ty), $container:ident ) => {
		paste! {
			pub fn $name(value: $type, threshold: $type) -> PrAnalysis {
				PrAnalysis::[<$name:camel>] { scoring: $container { value, threshold }}
			}
		}
	};
}

macro_rules! pr_count_constructor {
	( $name:tt ) => {
		pr_constructor!($name(u64), Count);
	};
}

// Unused for now
#[allow(unused_macros)]
macro_rules! pr_percent_constructor {
	( $name:tt ) => {
		pr_constructor!($name(f64), Percent);
	};
}

impl PrAnalysis {
	pr_count_constructor!(pr_affiliation);
	pr_percent_constructor!(pr_contributor_trust);
	pr_percent_constructor!(pr_module_contributors);

	/// Get the name of the analysis, for printing.
	pub fn name(&self) -> &'static str {
		match self {
			PrAnalysis::PrAffiliation { .. } => "pull request affiliation",
			PrAnalysis::PrContributorTrust { .. } => "pull request contributor trust",
			PrAnalysis::PrModuleContributors { .. } => "pull request module contributors",
		}
	}

	pub fn is_passing(&self) -> bool {
		use PrAnalysis::*;

		match self {
			PrAffiliation {
				scoring: Count { value, threshold },
			} => value <= threshold,
			PrContributorTrust {
				scoring: Percent { value, threshold },
			} => value >= threshold,
			PrModuleContributors {
				scoring: Percent { value, threshold },
			} => value <= threshold,
		}
	}

	pub fn permits_some_concerns(&self) -> bool {
		use PrAnalysis::*;

		match self {
			PrAffiliation {
				scoring: Count { threshold, .. },
			} => *threshold != 0,
			PrContributorTrust {
				scoring: Percent { threshold, .. },
			} => *threshold != 0.0,
			PrModuleContributors {
				scoring: Percent { threshold, .. },
			} => *threshold != 0.0,
		}
	}

	pub fn statement(&self) -> String {
		use PrAnalysis::*;

		String::from(match self {
			PrAffiliation { .. } => match (self.is_passing(), self.permits_some_concerns()) {
				(true, true) => "few concerning contributors",
				(true, false) => "no concerning contributors",
				(false, true) => "too many concerning contributors",
				(false, false) => "has concerning contributors",
			},
			PrContributorTrust { .. } => match (self.is_passing(), self.permits_some_concerns()) {
				(true, true) => "few untrusted contributors",
				(true, false) => "no untrusted contributors",
				(false, true) => "too many untrusted contributors",
				(false, false) => "has untrusted contributors",
			},
			PrModuleContributors { .. } => {
				if self.is_passing() {
					"Few enough of this pull request's contributors are contributing to new modules."
				} else {
					"Too many of this pull request's contributors are contributing to new modules."
				}
			}
		})
	}

	pub fn explanation(&self) -> String {
		use PrAnalysis::*;

		match self {
			PrAffiliation {
				scoring: Count { value, threshold },
			} => format!("{} found, {} permitted", value, threshold),
			PrContributorTrust {
				scoring: Percent { value, threshold },
			} => format!(
				"{:.2}% of contributors are trusted, at least {:.2}% must be trusted",
				value * 100.0,
				threshold * 100.0
			),
			PrModuleContributors {
				scoring: Percent { value, threshold },
			} => format!(
				"{:.2}% of contributors are comming to new modules; no more than {:.2}% can contribute to new modules",
				value * 100.0,
				threshold * 100.0
			),
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
#[schemars(crate = "self::schemars")]
pub enum PrConcern {
	/// Commits with affiliated contributor(s)
	PrAffiliation { contributor: String, count: i64 },
	/// Commits with untrusted contributor(s)
	PrContributorTrust { contributor: String },
}

impl PrConcern {
	/// Check if a concern is a pull request affiliation concern.
	fn is_pr_affiliation_concern(&self) -> bool {
		matches!(self, PrConcern::PrAffiliation { .. })
	}

	/// Check if a concern is a pull request contributor trust concern.
	fn is_pr_contributor_trust_concern(&self) -> bool {
		matches!(self, PrConcern::PrContributorTrust { .. })
	}

	pub fn description(&self) -> String {
		use PrConcern::*;

		match self {
			PrAffiliation { contributor, count } => {
				format!("{} - affiliation count {}", contributor, count)
			}
			PrContributorTrust { contributor } => {
				format!("untrusted contributor: {}", contributor)
			}
		}
	}
}

impl Hash for PrConcern {
	fn hash<H: Hasher>(&self, state: &mut H) {
		match self {
			PrConcern::PrAffiliation { contributor, .. } => contributor.hash(state),
			PrConcern::PrContributorTrust { contributor, .. } => contributor.hash(state),
		}
	}
}

// Currently there are no concerns that could be equal. Leaving this impl here for later use
impl PartialEq for PrConcern {
	fn eq(&self, _other: &PrConcern) -> bool {
		false
	}
}

impl Eq for PrConcern {}

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
#[schemars(crate = "self::schemars")]
pub struct Count {
	value: u64,
	threshold: u64,
}

/// Value for binary-based analyses.
#[derive(Debug, Serialize, JsonSchema, Clone, Copy)]
#[schemars(crate = "self::schemars")]
pub struct Exists {
	value: bool,
}

/// Value and threshold for percentage-based analyses.
#[derive(Debug, Serialize, JsonSchema, Clone, Copy)]
#[schemars(crate = "self::schemars")]
pub struct Percent {
	value: f64,
	threshold: f64,
}

/// A final recommendation of whether to use or investigate a piece of software,
/// including the risk threshold associated with that decision.
#[derive(Debug, Serialize, JsonSchema, Clone, Copy)]
#[schemars(crate = "self::schemars")]
pub struct Recommendation {
	pub kind: RecommendationKind,
	risk_score: RiskScore,
	risk_threshold: RiskThreshold,
}

impl Recommendation {
	/// Make a recommendation.
	pub fn is(risk_score: RiskScore, risk_threshold: RiskThreshold) -> Recommendation {
		let kind = RecommendationKind::is(risk_score, risk_threshold);

		Recommendation {
			kind,
			risk_score,
			risk_threshold,
		}
	}

	pub fn statement(&self) -> String {
		format!(
			"risk rated as {:.2}, acceptable below or equal to {:.2}",
			self.risk_score.0, self.risk_threshold.0
		)
	}
}

/// The kind of recommendation being made.
#[derive(Debug, Serialize, JsonSchema, Clone, Copy)]
#[schemars(crate = "self::schemars")]
pub enum RecommendationKind {
	Pass,
	Investigate,
}

impl RecommendationKind {
	fn is(risk_score: RiskScore, risk_threshold: RiskThreshold) -> RecommendationKind {
		if risk_score.0 > risk_threshold.0 {
			RecommendationKind::Investigate
		} else {
			RecommendationKind::Pass
		}
	}
}

/// The overall final risk score for a repo.
#[derive(Debug, Serialize, JsonSchema, Clone, Copy)]
#[serde(transparent)]
#[schemars(crate = "self::schemars")]
pub struct RiskScore(pub f64);

/// The risk threshold configured for the Hipcheck session.
#[derive(Debug, Serialize, JsonSchema, Clone, Copy)]
#[serde(transparent)]
#[schemars(crate = "self::schemars")]
pub struct RiskThreshold(pub f64);

/// A serializable and printable wrapper around a datetime with the local timezone.
#[derive(Debug, JsonSchema)]
#[schemars(crate = "self::schemars")]
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
