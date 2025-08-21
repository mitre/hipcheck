// SPDX-License-Identifier: Apache-2.0

use crate::policy_exprs::F64;
use crate::policy_exprs::error::JiffError;
use jiff::{
	Span, SpanCompare, Timestamp, Zoned,
	civil::{Date, DateTime},
	tz::TimeZone,
};
use logos::{Lexer, Logos};
use ordered_float::FloatIsNan;
use std::{
	cmp::Ordering,
	fmt::Display,
	num::{ParseFloatError, ParseIntError},
};

type Result<T> = std::result::Result<T, LexingError>;

#[derive(Logos, Clone, Debug)]
#[logos(skip r"[ \t\n\f]+", error = LexingError)]
pub enum Token {
	#[token("(")]
	OpenParen,

	#[token(")")]
	CloseParen,

	#[token("[")]
	OpenBrace,

	#[token("]")]
	CloseBrace,

	#[regex(r"\#[tf]", lex_bool)]
	Bool(bool),

	#[regex(r"-?(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?", lex_float)]
	Float(F64),

	#[regex(r"([1-9]?[0-9]*)", lex_integer, priority = 20)]
	Integer(i64),

	#[regex(r"[0-9]{3,4}-[^\s\)]+", lex_datetime)]
	DateTime(Box<Zoned>),

	// In the future this regex *could* be made more specific to reduce collision
	// with Ident, or we could introduce a special prefix character like '@' or '#'
	#[regex(r"PT?[0-9.]+[a-zA-Z][^\s\)]*", lex_span)]
	Span(Box<Span>),

	// Prioritize over span regex, which starts with a 'P'
	#[regex(r"([a-zA-Z]+)", lex_ident, priority = 10)]
	Ident(String),

	#[regex(r"\$[/~_[:alnum:]]*", lex_json_pointer)]
	JSONPointer(String),
}

impl PartialEq for Token {
	fn eq(&self, other: &Self) -> bool {
		match (self, other) {
			(Self::Bool(l0), Self::Bool(r0)) => l0 == r0,
			(Self::Float(l0), Self::Float(r0)) => l0 == r0,
			(Self::Integer(l0), Self::Integer(r0)) => l0 == r0,
			(Self::DateTime(l0), Self::DateTime(r0)) => l0 == r0,
			(Self::Span(l0), Self::Span(r0)) => {
				let r0 = SpanCompare::from(**r0).days_are_24_hours();
				l0.compare(r0).expect("span's must be comparable") == Ordering::Equal
			}
			(Self::Ident(l0), Self::Ident(r0)) => l0 == r0,
			(Self::JSONPointer(l0), Self::JSONPointer(r0)) => l0 == r0,
			_ => core::mem::discriminant(self) == core::mem::discriminant(other),
		}
	}
}

/// Lex a single boolean.
fn lex_bool(input: &mut Lexer<'_, Token>) -> Result<bool> {
	match input.slice() {
		"#t" => Ok(true),
		"#f" => Ok(false),
		value => Err(LexingError::InvalidBool(String::from(value))),
	}
}

/// Lex a single integer.
fn lex_integer(input: &mut Lexer<'_, Token>) -> Result<i64> {
	let s = input.slice();
	let i = s
		.parse::<i64>()
		.map_err(|err| LexingError::InvalidInteger(s.to_string(), err))?;
	Ok(i)
}

/// Lex a single float.
fn lex_float(input: &mut Lexer<'_, Token>) -> Result<F64> {
	let s = input.slice();
	let f = s
		.parse::<f64>()
		.map_err(|err| LexingError::InvalidFloat(s.to_string(), err))?;
	Ok(F64::new(f)?)
}

/// Lex a single datetime value.
fn lex_datetime(input: &mut Lexer<'_, Token>) -> Result<Box<Zoned>> {
	let s = input.slice();
	// Parse to a Zoned datetime value with as much detail as given
	// If a UTC offset is provided, convert the datetime to the equivalent UTC datetime
	if let Ok(timestamp) = s.parse::<Timestamp>() {
		Ok(Box::new(timestamp.to_zoned(TimeZone::UTC)))
	// If no offset is provided, assume the time is UTC
	} else if let Ok(dt) = s.parse::<DateTime>() {
		dt.to_zoned(TimeZone::UTC)
			.map_err(|err| LexingError::InvalidDatetime(s.to_string(), JiffError::new(err)))
			.map(Box::new)
	} else {
		match s.parse::<Date>() {
			// If no time is provided, treat the time as midnight UTC on the given day
			Ok(date) => date
				.to_zoned(TimeZone::UTC)
				.map_err(|err| LexingError::InvalidDatetime(s.to_string(), JiffError::new(err)))
				.map(Box::new),
			// If the string provided does not parse to a valid date or datetime, return an error
			Err(err) => Err(LexingError::InvalidDatetime(
				s.to_string(),
				JiffError::new(err),
			)),
		}
	}
}

/// Lex a time span
fn lex_span(input: &mut Lexer<'_, Token>) -> Result<Box<Span>> {
	let s = input.slice();
	s.parse::<Span>()
		.map_err(|err| LexingError::InvalidSpan(s.to_string(), JiffError::new(err)))
		.map(|x| span_to_days(&x))?
		.map(Box::new)
}

// Error if the span contains years or months.
// If the span contains weeks, convert the weeks to days by treating every week as 7 24-hour days.
fn span_to_days(full_span: &Span) -> Result<Span> {
	if full_span.get_years() != 0 || full_span.get_months() != 0 {
		Err(LexingError::SpanWithBadUnits)
	} else {
		let weeks = full_span.get_weeks();
		let days = full_span.get_days();
		let total_days = weeks * 7 + days;

		// Panic: The unwrap() on try_weeks will not panic when the argument is 0.
		full_span
			.try_weeks(0)
			.unwrap()
			.try_days(total_days)
			.map_err(|err| LexingError::InvalidSpan(full_span.to_string(), JiffError::new(err)))
	}
}

/// Lex a single identifier.
fn lex_ident(input: &mut Lexer<'_, Token>) -> Result<String> {
	Ok(input.slice().to_owned())
}

/// Lex a JSON Pointer.
/// The initial '$' character is removed.
fn lex_json_pointer(input: &mut Lexer<'_, Token>) -> Result<String> {
	let token = input.slice();
	// Remove the initial '$' character.
	let pointer = token.get(1..).ok_or(LexingError::InternalError(format!(
		"JSON Pointer token missing mandatory initial '$': got '{}'",
		token
	)))?;
	if let Some(chr) = pointer.chars().next()
		&& chr != '/'
	{
		return Err(LexingError::JSONPointerMissingInitialSlash(
			pointer.to_owned(),
		));
	}
	Ok(pointer.to_owned())
}

impl Display for Token {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Token::OpenParen => write!(f, "("),
			Token::CloseParen => write!(f, ")"),
			Token::OpenBrace => write!(f, "["),
			Token::CloseBrace => write!(f, "]"),
			Token::Bool(true) => write!(f, "#t"),
			Token::Bool(false) => write!(f, "#f"),
			Token::Integer(i) => write!(f, "{i}"),
			Token::Float(fl) => write!(f, "{fl}"),
			Token::DateTime(dt) => write!(f, "{dt}"),
			Token::Span(span) => write!(f, "{span}"),
			Token::Ident(i) => write!(f, "{i}"),
			Token::JSONPointer(pointer) => write!(f, "${pointer}"),
		}
	}
}

/// Error arising during lexing.
#[derive(Default, Debug, Clone, PartialEq, thiserror::Error)]
pub enum LexingError {
	#[error("an unknown lexing error occured")]
	#[default]
	UnknownError,

	#[error("internal error: '{0}'")]
	InternalError(String),

	#[error("failed to parse integer")]
	InvalidInteger(String, ParseIntError),

	#[error("failed to parse float")]
	InvalidFloat(String, ParseFloatError),

	#[error("float is not a number")]
	FloatIsNan(#[from] FloatIsNan),

	#[error("invalid boolean, found '{0}'")]
	InvalidBool(String),

	#[error("invalid JSON Pointer, found '{0}'. JSON Pointers must be empty or start with '/'.")]
	JSONPointerMissingInitialSlash(String),

	#[error("failed to parse date or datetime")]
	InvalidDatetime(String, JiffError),

	#[error("failed to parse span")]
	InvalidSpan(String, JiffError),

	#[error("span cannot contain units of years or months")]
	SpanWithBadUnits,
}

#[cfg(test)]
mod tests {
	use crate::policy_exprs::{Error::Lex, F64, LexingError, Result, token::Token};
	use jiff::{Span, Timestamp, Zoned, tz::TimeZone};
	use logos::Logos as _;
	use test_log::test;

	// Helper function for running the lexer to get all tokens.
	fn lex(input: &str) -> Result<Vec<Token>> {
		let tokens = Token::lexer(input)
			.map(|res| res.map_err(Into::into))
			.collect::<Result<Vec<_>>>()?;
		Ok(tokens)
	}

	#[test]
	fn basic_lexing() {
		let raw_program = "(add 1 2)";
		let expected = vec![
			Token::OpenParen,
			Token::Ident(String::from("add")),
			Token::Integer(1),
			Token::Integer(2),
			Token::CloseParen,
		];
		let tokens = lex(raw_program).unwrap();
		assert_eq!(tokens, expected);
	}

	#[test]
	fn basic_lexing_with_floats() {
		let raw_program = "(add 1.0 2.0)";
		let expected = vec![
			Token::OpenParen,
			Token::Ident(String::from("add")),
			Token::Float(F64::new(1.0).unwrap()),
			Token::Float(F64::new(2.0).unwrap()),
			Token::CloseParen,
		];
		let tokens = lex(raw_program).unwrap();
		assert_eq!(tokens, expected);
	}

	#[test]
	fn basic_lexing_with_bools() {
		let raw_program = "(eq #t #f)";
		let expected = vec![
			Token::OpenParen,
			Token::Ident(String::from("eq")),
			Token::Bool(true),
			Token::Bool(false),
			Token::CloseParen,
		];
		let tokens = lex(raw_program).unwrap();
		assert_eq!(tokens, expected);
	}

	#[test]
	fn basic_lexing_with_jsonptr_empty() {
		let raw_program = "$";
		let expected = vec![
			// Note that the initial '$' is not considered part of the pointer string.
			Token::JSONPointer(String::from("")),
		];
		let tokens = lex(raw_program).unwrap();
		assert_eq!(tokens, expected);
	}

	#[test]
	fn basic_lexing_with_jsonptr_error_invalid() {
		// This JSON Pointer is invalid because it doesn't start with a '/' character.
		let raw_program = "$alpha";
		let expected = Err(Lex(LexingError::JSONPointerMissingInitialSlash(
			String::from("alpha"),
		)));
		let tokens = lex(raw_program);
		assert_eq!(tokens, expected);
	}

	#[test]
	fn basic_lexing_with_jsonptr_valid_chars() {
		let raw_program = "$/alpha_bravo/~0/~1";
		let expected = vec![Token::JSONPointer(String::from("/alpha_bravo/~0/~1"))];
		let tokens = lex(raw_program).unwrap();
		assert_eq!(tokens, expected);
	}

	#[test]
	fn basic_lexing_with_jsonptr_in_expr() {
		let raw_program = "(eq 1 $/data/one)";
		let expected = vec![
			Token::OpenParen,
			Token::Ident(String::from("eq")),
			Token::Integer(1),
			Token::JSONPointer(String::from("/data/one")),
			Token::CloseParen,
		];

		let tokens = lex(raw_program).unwrap();
		assert_eq!(tokens, expected);
	}

	#[test]
	fn basic_lexing_with_time() {
		let raw_program = "(eq (duration 2024-09-17T09:00-05 2024-09-17T10:30-05) PT1H30M)";

		let ts1: Timestamp = "2024-09-17T09:00-05".parse().unwrap();
		let dt1 = Zoned::new(ts1, TimeZone::UTC);
		let ts2: Timestamp = "2024-09-17T10:30-05".parse().unwrap();
		let dt2 = Zoned::new(ts2, TimeZone::UTC);
		let span: Span = "PT1H30M".parse().unwrap();

		let expected = vec![
			Token::OpenParen,
			Token::Ident(String::from("eq")),
			Token::OpenParen,
			Token::Ident(String::from("duration")),
			Token::DateTime(Box::new(dt1)),
			Token::DateTime(Box::new(dt2)),
			Token::CloseParen,
			Token::Span(Box::new(span)),
			Token::CloseParen,
		];

		let tokens = lex(raw_program).unwrap();
		assert_eq!(tokens, expected);
	}

	#[test]
	fn lexing_with_bad_span() {
		let raw_program = "P4M2W4DT1H30M";
		let expected = Err(Lex(LexingError::SpanWithBadUnits));
		let tokens = lex(raw_program);
		assert_eq!(tokens, expected);
	}

	// Ensure that idents with capital P are prioritized over being treated as spans
	#[test]
	fn regression_lex_span_and_ident() {
		let raw_program = "Philip";
		let expected = vec![Token::Ident(String::from("Philip"))];

		let tokens = lex(raw_program).unwrap();
		assert_eq!(tokens, expected);

		let raw_program = "PT1H30M";
		let span: Span = raw_program.parse().unwrap();

		let expected = vec![Token::Span(Box::new(span))];

		let tokens = lex(raw_program).unwrap();
		assert_eq!(tokens, expected);

		let raw_program = "PTBarnum";
		let expected = vec![Token::Ident(String::from("PTBarnum"))];

		let tokens = lex(raw_program).unwrap();
		assert_eq!(tokens, expected);
	}
}
