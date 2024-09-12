// SPDX-License-Identifier: Apache-2.0

use crate::policy_exprs::F64;
use logos::{Lexer, Logos};
use ordered_float::FloatIsNan;
use std::{
	fmt::Display,
	num::{ParseFloatError, ParseIntError},
};

type Result<T> = std::result::Result<T, LexingError>;

#[derive(Logos, Clone, Debug, PartialEq)]
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

	#[regex("([a-zA-Z]+)", lex_ident)]
	Ident(String),

	#[regex(r"\$[/~_[:alnum:]]*", lex_json_pointer)]
	JSONPointer(String),
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

/// Lex a single identifier.
fn lex_ident(input: &mut Lexer<'_, Token>) -> Result<String> {
	Ok(input.slice().to_owned())
}

/// Lex a JSON Pointer.
fn lex_json_pointer(input: &mut Lexer<'_, Token>) -> Result<String> {
	let token = input.slice();
	// Remove the initial '$' character.
	let pointer = token.get(1..).ok_or(LexingError::InternalError(format!(
		"JSON Pointer token missing mandatory initial '$': got '{}'",
		token
	)))?;
	if let Some(chr) = pointer.chars().next() {
		if chr != '/' {
			return Err(LexingError::JSONPointerMissingInitialSlash(
				pointer.to_owned(),
			));
		}
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
}

#[cfg(test)]
mod tests {
	use crate::policy_exprs::{token::Token, Error::Lex, LexingError, Result, F64};
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
}
