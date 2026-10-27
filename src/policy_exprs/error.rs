// SPDX-License-Identifier: Apache-2.0

use crate::policy_exprs::{
	Expr, Ident, LexingError,
	expr::{PrimitiveType, Type},
};
use jiff::Error as JError;
use nom::{Needed, error::ErrorKind};
use ordered_float::FloatIsNan;
use std::fmt;

/// `Result` which uses [`Error`].
pub type Result<T> = std::result::Result<T, Error>;

/// An error arising during program execution.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum Error {
	#[error("Multiple errors: {0:?}")]
	MultipleErrors(Vec<Error>),

	#[error("internal error: '{0}'")]
	#[allow(clippy::enum_variant_names)]
	InternalError(Box<str>),

	#[error("missing close paren")]
	MissingOpenParen,

	#[error("missing open paren")]
	MissingCloseParen,

	#[error("missing ident")]
	MissingIdent,

	#[error("wrong type in ident spot")]
	WrongTypeInIdentSpot,

	#[error("missing args")]
	MissingArgs,

	#[error(transparent)]
	Lex(#[from] LexingError),

	#[error("expression returned '{0:?}', not a boolean")]
	DidNotReturnBool(Box<Expr>),

	#[error("evaluation of inner expression returned '{0:?}', not a primitive")]
	BadReturnType(Box<Expr>),

	#[error("tried to call unknown function '{0}'")]
	UnknownFunction(Box<str>),

	#[error("ident '{0}' resolved to a variable, not a function")]
	FoundVarExpectedFunc(Box<str>),

	#[error("parsing did not consume the entire input {}", needed_str(.0))]
	IncompleteParse(Needed),

	#[error("parse failed with kind '{kind:?}', with '{remaining}' remaining")]
	Parse {
		remaining: Box<str>,
		kind: ErrorKind,
	},

	#[error(transparent)]
	FloatIsNan(#[from] FloatIsNan),

	#[error("too many args to '{name}'; expected {expected}, got {given}")]
	TooManyArgs {
		name: Box<str>,
		expected: usize,
		given: usize,
	},

	#[error("not enough args to '{name}'; expected {expected}, got {given}")]
	NotEnoughArgs {
		name: Box<str>,
		expected: usize,
		given: usize,
	},

	#[error("called '{0}' with mismatched types")]
	BadType(&'static str),

	#[error("call to '{name}' with '{got:?}' as argument {idx}, expected {expected}")]
	BadFuncArgType {
		name: Box<str>,
		idx: usize,
		expected: Box<str>,
		got: Box<Type>,
	},

	#[error("array of {expected:?}s contains a {got:?} at idx {idx}")]
	BadArrayElt {
		idx: usize,
		expected: PrimitiveType,
		got: PrimitiveType,
	},

	#[error("no max value found in array")]
	NoMax,

	#[error("no min value found in array")]
	NoMin,

	#[error("no avg value found for array")]
	NoAvg,

	#[error("no median value found for array")]
	NoMedian,

	#[error("array mixing multiple primitive types")]
	InconsistentArrayTypes,

	#[error("variable '{0}' is not bound")]
	UnboundVar(Ident),

	#[error("variable '{0}' conflicts with function")]
	VarConflictsWithFunc(Ident),

	#[error("variable '{checked}' resolves to another variable '{found}'")]
	VarResolvesToVar { checked: Ident, found: Ident },

	#[error("variable is already bound")]
	AlreadyBound,

	#[error(
		"JSON Pointer invalid syntax: non-empty pointer must start with '/'. \
		pointer: '{pointer}'"
	)]
	JSONPointerInvalidSyntax { pointer: Box<str> },

	#[error("JSON Pointer lookup failed. pointer: '{pointer}'; context: {context}")]
	JSONPointerLookupFailed {
		pointer: Box<str>,
		context: Box<serde_json::Value>,
	},

	#[error(
		"JSON Pointer lookup returned a value whose type \
		is unrepresentable in Policy Expressions ({json_type:?}). \
		pointer: '{pointer}'; value: {value}; context: {context}"
	)]
	JSONPointerUnrepresentableType {
		json_type: UnrepresentableJSONType,
		pointer: Box<str>,
		value: Box<serde_json::Value>,
		context: Box<serde_json::Value>,
	},

	#[error("Datetime error: {0}")]
	Datetime(Box<str>),
}

#[derive(Debug, PartialEq)]
pub enum UnrepresentableJSONType {
	NonPrimitiveInArray,
	JSONObject,
	JSONString,
	JSONNull,
}

// Custom error to handle jiff's native error not impl PartialEq
// We exploit the fact that it *does* impl Display
#[derive(Clone, Debug, thiserror::Error, PartialEq)]
pub struct JiffError {
	jiff_error: String,
}

impl JiffError {
	pub fn new(err: JError) -> Self {
		let msg = err.to_string();
		JiffError { jiff_error: msg }
	}
}

impl fmt::Display for JiffError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{}", self.jiff_error)
	}
}

fn needed_str(needed: &Needed) -> String {
	match needed {
		Needed::Unknown => String::from(""),
		Needed::Size(bytes) => format!(", needed {} more bytes", bytes),
	}
}
