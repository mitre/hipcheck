// SPDX-License-Identifier: Apache-2.0

use crate::analysis::score::HCStoredResult;
use crate::hc_error;
use crate::report::Concern;
use crate::Result;
use crate::F64;
use std::cmp::Ordering;
use std::fmt::{self, Display};
use std::rc::Rc;

/// Represents the enhanced result of a hipcheck analysis. Contains the actual outcome
/// of the analysis, plus additional meta-information the analysis wants to provide to
/// Hipcheck core, such as raised concerns.
#[derive(Debug, Eq, PartialEq)]
pub struct HCAnalysisReport {
	pub outcome: HCAnalysisOutcome,
	pub concerns: Vec<Concern>,
}
impl HCAnalysisReport {
	pub fn generic_error(error: crate::error::Error, concerns: Vec<Concern>) -> Self {
		HCAnalysisReport {
			outcome: HCAnalysisOutcome::Error(HCAnalysisError::Generic(error)),
			concerns,
		}
	}
}

/// Represents the result of a hipcheck analysis. Either the analysis encountered
/// an error, or it completed and returned a value.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum HCAnalysisOutcome {
	Error(HCAnalysisError),
	Completed(HCAnalysisValue),
}

/// Enumeration of potential errors that a Hipcheck analysis might return. The Generic
/// variant enables representing errors that aren't covered by other variants.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum HCAnalysisError {
	Generic(crate::error::Error),
}

/// A Hipcheck analysis may return a basic or composite value. By splitting the types
/// into two sub-enums under this one, we can eschew a recursive enum definition and
/// ensure composite types only have a depth of one.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum HCAnalysisValue {
	Basic(HCBasicValue),
	Composite(HCCompositeValue),
}

/// Basic Hipcheck analysis return types
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum HCBasicValue {
	Integer(i64),
	Unsigned(u64),
	Float(F64),
	Bool(bool),
	String(String),
}
impl From<i64> for HCBasicValue {
	fn from(value: i64) -> Self {
		HCBasicValue::Integer(value)
	}
}
impl From<u64> for HCBasicValue {
	fn from(value: u64) -> Self {
		HCBasicValue::Unsigned(value)
	}
}
impl From<F64> for HCBasicValue {
	fn from(value: F64) -> Self {
		HCBasicValue::Float(value)
	}
}
impl TryFrom<f64> for HCBasicValue {
	type Error = crate::Error;
	fn try_from(value: f64) -> Result<HCBasicValue> {
		let inner = F64::new(value)?;
		Ok(HCBasicValue::Float(inner))
	}
}
impl From<bool> for HCBasicValue {
	fn from(value: bool) -> Self {
		HCBasicValue::Bool(value)
	}
}
impl From<&str> for HCBasicValue {
	fn from(value: &str) -> Self {
		HCBasicValue::String(value.to_owned())
	}
}
impl Display for HCBasicValue {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		use HCBasicValue::*;
		match self {
			Unsigned(u) => u.fmt(f),
			Integer(i) => i.fmt(f),
			String(s) => s.fmt(f),
			Float(fp) => fp.fmt(f),
			Bool(b) => b.fmt(f),
		}
	}
}

/// Composite Hipcheck analysis return types
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum HCCompositeValue {
	List(Vec<HCBasicValue>),
	Dict(indexmap::IndexMap<String, HCBasicValue>),
}

/// The set of possible predicates for deciding if a source passed an analysis.
pub trait HCPredicate: Display + std::fmt::Debug + std::any::Any + 'static {
	fn pass(&self) -> Result<bool>;
	fn as_any(&self) -> &dyn std::any::Any;
}

/// This predicate determines analysis pass/fail by whether a returned value was
/// greater than, less than, or equal to a target value.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ThresholdPredicate {
	pub value: HCBasicValue,
	pub threshold: HCBasicValue,
	units: String,
	pub ordering: Ordering,
}
impl ThresholdPredicate {
	pub fn new(
		value: HCBasicValue,
		threshold: HCBasicValue,
		units: Option<String>,
		ordering: Ordering,
	) -> Self {
		ThresholdPredicate {
			value,
			threshold,
			units: units.unwrap_or("".to_owned()),
			ordering,
		}
	}
}

fn pass_threshold<T: Ord>(a: &T, b: &T, ord: Ordering) -> bool {
	a.cmp(b) == ord
}

impl ThresholdPredicate {
	pub fn from_analysis(
		report: &HCAnalysisReport,
		threshold: HCBasicValue,
		units: Option<String>,
		order: Ordering,
	) -> HCStoredResult {
		let result = match &report.outcome {
			HCAnalysisOutcome::Error(err) => Err(hc_error!("{:?}", err)),
			HCAnalysisOutcome::Completed(HCAnalysisValue::Basic(av)) => {
				Ok(ThresholdPredicate::new(av.clone(), threshold, units, order))
			}
			HCAnalysisOutcome::Completed(HCAnalysisValue::Composite(_)) => Err(hc_error!(
				"activity analysis should return a basic u64 type, not {:?}"
			)),
		};
		HCStoredResult {
			result: result.map(|r| Rc::new(r) as Rc<dyn HCPredicate>),
			concerns: report.concerns.clone(),
		}
	}
}
impl HCPredicate for ThresholdPredicate {
	// @FollowUp - would be nice for this match logic to error at compile time if a new
	//  HCBasicValue type is added, so developer is reminded to add new variant here
	fn pass(&self) -> Result<bool> {
		use HCBasicValue::*;
		match (&self.value, &self.threshold) {
			(Integer(a), Integer(b)) => Ok(pass_threshold(a, b, self.ordering)),
			(Unsigned(a), Unsigned(b)) => Ok(pass_threshold(a, b, self.ordering)),
			(Float(a), Float(b)) => Ok(pass_threshold(a, b, self.ordering)),
			(Bool(a), Bool(b)) => Ok(pass_threshold(a, b, self.ordering)),
			(String(a), String(b)) => Ok(pass_threshold(a, b, self.ordering)),
			(a, b) => Err(hc_error!(
				"threshold and value are of different types: {:?}, {:?}",
				a,
				b
			)),
		}
	}
	fn as_any(&self) -> &dyn std::any::Any {
		self
	}
}
impl Display for ThresholdPredicate {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		use Ordering::*;
		// append units. if none, trim() call below will clean up whitespace
		let val = format!("{} {}", self.value, &self.units);
		let thr = format!("{} {}", self.threshold, &self.units);
		let order_str = match &self.ordering {
			Less => "<",
			Equal => "==",
			Greater => ">",
		};
		write!(f, "{} {} {}", val.trim(), order_str, thr.trim())
	}
}
