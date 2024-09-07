// SPDX-License-Identifier: Apache-2.0

use crate::policy_exprs::{Expr, Ident, LexingError};
use nom::{error::ErrorKind, Needed};
use ordered_float::FloatIsNan;

/// `Result` which uses [`Error`].
pub type Result<T> = std::result::Result<T, Error>;

/// An error arising during program execution.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum Error {
	#[error("Multiple errors: {0:?}")]
	MultipleErrors(Vec<Error>),

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
	DidNotReturnBool(Expr),

	#[error("tried to call unknown function '{0}'")]
	UnknownFunction(String),

	#[error("ident '{0}' resolved to a variable, not a function")]
	FoundVarExpectedFunc(String),

	#[error("parsing did not consume the entire input {}", needed_str(.0))]
	IncompleteParse(Needed),

	#[error("parse failed with kind '{kind:?}', with '{remaining}' remaining")]
	Parse { remaining: String, kind: ErrorKind },

	#[error(transparent)]
	FloatIsNan(#[from] FloatIsNan),

	#[error("too many args to '{name}'; expected {expected}, got {given}")]
	TooManyArgs {
		name: String,
		expected: usize,
		given: usize,
	},

	#[error("not enough args to '{name}'; expected {expected}, got {given}")]
	NotEnoughArgs {
		name: String,
		expected: usize,
		given: usize,
	},

	#[error("called '{0}' with mismatched types")]
	BadType(&'static str),

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
	JSONPointerInvalidSyntax { pointer: String },

	#[error("JSON Pointer lookup failed. pointer: '{pointer}'; context: {context}")]
	JSONPointerLookupFailed {
		pointer: String,
		context: serde_json::Value,
	},

	#[error(
		"JSON Pointer lookup returned a value whose type \
		is unrepresentable in Policy Expressions ({json_type:?}). \
		pointer: '{pointer}'; value: {value}; context: {context}"
	)]
	JSONPointerUnrepresentableType {
		json_type: UnrepresentableJSONType,
		pointer: String,
		value: serde_json::Value,
		context: serde_json::Value,
	},
}

#[derive(Debug, PartialEq)]
pub enum UnrepresentableJSONType {
	NonPrimitiveInArray,
	JSONObject,
	JSONString,
	JSONNull,
}

fn needed_str(needed: &Needed) -> String {
	match needed {
		Needed::Unknown => String::from(""),
		Needed::Size(bytes) => format!(", needed {} more bytes", bytes),
	}
}
