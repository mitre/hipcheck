use crate::policy_exprs::{Expr, Ident, LexingError};
use nom::{error::ErrorKind, Needed};
use ordered_float::FloatIsNan;

/// `Result` which uses [`Error`].
pub type Result<T> = std::result::Result<T, Error>;

/// An error arising during program execution.
#[derive(Debug, thiserror::Error)]
pub enum Error {
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
}

fn needed_str(needed: &Needed) -> String {
	match needed {
		Needed::Unknown => String::from(""),
		Needed::Size(bytes) => format!(", needed {} more bytes", bytes),
	}
}
