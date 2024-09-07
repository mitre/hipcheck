// SPDX-License-Identifier: Apache-2.0

use crate::policy_exprs::F64;
use logos::Lexer;
use logos::Logos;
use ordered_float::FloatIsNan;
use std::fmt::Display;
use std::num::ParseFloatError;
use std::num::ParseIntError;

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
		}
	}
}

/// Error arising during lexing.
#[derive(Default, Debug, Clone, PartialEq, thiserror::Error)]
pub enum LexingError {
	#[error("an unknown lexing error occured")]
	#[default]
	UnknownError,

	#[error("failed to parse integer")]
	InvalidInteger(String, ParseIntError),

	#[error("failed to parse float")]
	InvalidFloat(String, ParseFloatError),

	#[error("float is not a number")]
	FloatIsNan(#[from] FloatIsNan),

	#[error("invalid boolean, found '{0}'")]
	InvalidBool(String),
}

#[cfg(test)]
mod tests {
	use crate::policy_exprs::token::Token;
	use crate::policy_exprs::Result;
	use crate::policy_exprs::F64;
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
}
