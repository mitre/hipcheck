use crate::hc_error;
use crate::report::Concern;
use crate::Result;
use crate::F64;

/// Represents the enhanced result of a hipcheck analysis. Contains the actual outcome
/// of the analysis, plus additional meta-information the analysis wants to provide to
/// HipCheck core, such as raised concerns.
#[allow(dead_code)]
pub struct HCAnalysisResult {
	pub outcome: AnalysisOutcome,
	pub concerns: Vec<Concern>,
}

#[allow(dead_code)]
pub type SkippableAnalysisResult = Option<HCAnalysisResult>;

/// Represents the result of a hipcheck analysis. Either the analysis encountered
/// an error, or it completed and returned a value.
#[allow(dead_code)]
pub enum AnalysisOutcome {
	Error(HCAnalysisError),
	Completed(HCAnalysisValue),
}

/// Enumeration of potential errors that a HipCheck analysis might return. The Generic
/// variant enables representing errors that aren't covered by other variants.
#[allow(dead_code)]
pub enum HCAnalysisError {
	Generic(crate::error::Error),
}

/// A HipCheck analysis may return a basic or composite value. By splitting the types
/// into two sub-enums under this one, we can eschew a recursive enum definition and
/// ensure composite types only have a depth of one.
#[allow(dead_code)]
pub enum HCAnalysisValue {
	Basic(HCBasicValue),
	Composite(HCCompositeValue),
}

/// Basic HipCheck analysis return types
#[derive(Debug)]
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
impl From<String> for HCBasicValue {
	fn from(value: String) -> Self {
		HCBasicValue::String(value)
	}
}
impl From<&str> for HCBasicValue {
	fn from(value: &str) -> Self {
		HCBasicValue::String(value.to_owned())
	}
}

/// Composite HipCheck analysis return types
#[allow(dead_code)]
pub enum HCCompositeValue {
	List(Vec<HCBasicValue>),
	Dict(indexmap::IndexMap<String, HCBasicValue>),
}

/// The set of possible predicates for deciding if a source passed an analysis.
#[allow(dead_code)]
pub enum HCPredicate {
	Threshold(ThresholdPredicate),
}

/// This predicate determines analysis pass/fail by whether a returned value was
/// greater than, less than, or equal to a target value.
#[allow(dead_code)]
pub struct ThresholdPredicate {
	pub value: HCBasicValue,
	pub threshold: HCBasicValue,
	pub ordering: std::cmp::Ordering,
}

fn pass_threshold<T: PartialOrd>(a: &T, b: &T, ord: &std::cmp::Ordering) -> Result<bool> {
	a.partial_cmp(b)
		.ok_or(hc_error!("threshold comparison failed for unknown reason"))
		.map(|x| x == *ord)
}

#[allow(dead_code)]
impl ThresholdPredicate {
	// @FollowUp - would be nice for this match logic to error at compile time if a new
	//  HCBasicValue type is added, so developer is reminded to add new variant here
	pub fn pass(&self) -> Result<bool> {
		use HCBasicValue::*;
		match (&self.value, &self.threshold) {
			(Integer(a), Integer(b)) => pass_threshold(a, b, &self.ordering),
			(Unsigned(a), Unsigned(b)) => pass_threshold(a, b, &self.ordering),
			(Float(a), Float(b)) => pass_threshold(a, b, &self.ordering),
			(Bool(a), Bool(b)) => pass_threshold(a, b, &self.ordering),
			(String(a), String(b)) => pass_threshold(a, b, &self.ordering),
			(a, b) => Err(hc_error!(
				"threshold and value are of different types: {:?}, {:?}",
				a,
				b
			)),
		}
	}
}
