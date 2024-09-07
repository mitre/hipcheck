// SPDX-License-Identifier: Apache-2.0

use crate::policy_exprs::{
	env::{Binding, Env},
	token::Token,
	Error, Result, Tokens,
};
use itertools::Itertools;
use nom::{
	branch::alt,
	combinator::{all_consuming, map},
	multi::many0,
	sequence::tuple,
	Finish as _, IResult,
};
use ordered_float::NotNan;
use std::{fmt::Display, ops::Deref};

/// A `deke` expression to evaluate.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Expr {
	/// Primitive data (ints, floats, bool).
	Primitive(Primitive),

	/// An array of primitive data.
	Array(Vec<Primitive>),

	/// Stores the name of the function, followed by the args.
	Function(Ident, Vec<Expr>),

	/// Stores the name of the input variable, followed by the lambda body.
	Lambda(Ident, Box<Expr>),
}

/// Primitive data.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Primitive {
	/// Identifier in a lambda, to be substituted.
	Identifier(Ident),

	/// Signed 64-bit integer.
	Int(i64),

	/// 64-bit float, not allowed to be NaN.
	Float(F64),

	/// Boolean.
	Bool(bool),
}

/// A variable or function identifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ident(pub String);

/// A non-NaN 64-bit floating point number.
pub type F64 = NotNan<f64>;

impl Display for Expr {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Expr::Primitive(primitive) => write!(f, "{}", primitive),
			Expr::Array(array) => {
				write!(f, "[{}]", array.iter().map(ToString::to_string).join(" "))
			}
			Expr::Function(ident, args) => {
				let args = args.iter().map(ToString::to_string).join(" ");
				write!(f, "({} {})", ident, args)
			}
			Expr::Lambda(arg, body) => write!(f, "(lambda ({}) {}", arg, body),
		}
	}
}

impl Display for Primitive {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Primitive::Identifier(ident) => write!(f, "{}", ident),
			Primitive::Int(i) => write!(f, "{}", i),
			Primitive::Float(fl) => write!(f, "{}", fl),
			Primitive::Bool(b) => write!(f, "{}", if *b { "#t" } else { "#f" }),
		}
	}
}

impl Primitive {
	pub fn resolve(&self, env: &Env) -> Result<Primitive> {
		match self {
			Primitive::Identifier(ident) => match env.get(ident) {
				Some(Binding::Var(Primitive::Identifier(found))) => Err(Error::VarResolvesToVar {
					checked: ident.clone(),
					found,
				}),
				Some(Binding::Var(var)) => Ok(var),
				Some(Binding::Fn(_)) => Err(Error::VarConflictsWithFunc(ident.clone())),
				None => Err(Error::UnboundVar(ident.clone())),
			},
			_ => Ok(self.clone()),
		}
	}
}

impl Display for Ident {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.0)
	}
}

impl Deref for Ident {
	type Target = str;

	fn deref(&self) -> &Self::Target {
		self.0.deref()
	}
}

crate::token_parser!(token: Token);

crate::data_variant_parser! {
	fn parse_integer(input) -> Result<Primitive>;
	pattern = Token::Integer(n) => Primitive::Int(n);
}

crate::data_variant_parser! {
	fn parse_float(input) -> Result<Primitive>;
	pattern = Token::Float(f) => Primitive::Float(f);
}

crate::data_variant_parser! {
	fn parse_bool(input) -> Result<Primitive>;
	pattern = Token::Bool(b) => Primitive::Bool(b);
}

crate::data_variant_parser! {
	fn parse_ident(input) -> Result<String>;
	pattern = Token::Ident(s) => s.to_owned();
}

// Helper type for token parsing.
pub type Input<'source> = Tokens<'source, Token>;

/// Parse a single piece of primitive data.
fn parse_primitive(input: Input<'_>) -> IResult<Input<'_>, Primitive> {
	alt((parse_integer, parse_float, parse_bool))(input)
}

/// Parse an array.
fn parse_array(input: Input<'_>) -> IResult<Input<'_>, Expr> {
	let parser = tuple((Token::OpenBrace, many0(parse_primitive), Token::CloseBrace));
	let mut parser = map(parser, |(_, inner, _)| Expr::Array(inner));
	parser(input)
}

/// Parse an expression.
fn parse_expr(input: Input<'_>) -> IResult<Input<'_>, Expr> {
	let primitive = map(parse_primitive, Expr::Primitive);
	alt((primitive, parse_array, parse_function))(input)
}

/// Parse a function call.
fn parse_function(input: Input<'_>) -> IResult<Input<'_>, Expr> {
	let parser = tuple((
		Token::OpenParen,
		parse_ident,
		many0(parse_expr),
		Token::CloseParen,
	));
	let mut parser = map(parser, |(_, ident, args, _)| {
		Expr::Function(Ident(ident), args)
	});
	parser(input)
}

pub fn parse(input: &str) -> Result<Expr> {
	let tokens = Tokens::new(input);
	let mut parser = all_consuming(parse_function);

	match parser(tokens).finish() {
		Ok((rest, expr)) if rest.is_empty() => Ok(expr),
		Ok(_) => Err(Error::IncompleteParse(nom::Needed::Unknown)),
		Err(err) => {
			let remaining = err.input.lexer().slice().to_string();
			let kind = err.code;
			Err(Error::Parse { remaining, kind })
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use test_log::test;

	trait IntoExpr {
		fn into_expr(self) -> Expr;
	}

	impl IntoExpr for Expr {
		fn into_expr(self) -> Expr {
			self
		}
	}

	impl IntoExpr for Primitive {
		fn into_expr(self) -> Expr {
			Expr::Primitive(self)
		}
	}

	fn func(name: &str, args: Vec<impl IntoExpr>) -> Expr {
		let args = args.into_iter().map(|arg| arg.into_expr()).collect();
		Expr::Function(Ident(String::from(name)), args)
	}

	fn int(val: i64) -> Primitive {
		Primitive::Int(val)
	}

	fn float(val: f64) -> Primitive {
		Primitive::Float(F64::new(val).unwrap())
	}

	fn boolean(val: bool) -> Primitive {
		Primitive::Bool(val)
	}

	fn array(vals: Vec<Primitive>) -> Expr {
		Expr::Array(vals)
	}

	#[test]
	fn parse_function() {
		let input = "(add 2 3)";
		let expected = func("add", vec![int(2), int(3)]);
		let result = parse(input).unwrap();
		assert_eq!(result, expected);
	}

	#[test]
	fn parse_nested_function() {
		let input = "(add (add 1 2) 3)";
		let expected = func(
			"add",
			vec![func("add", vec![int(1), int(2)]), int(3).into_expr()],
		);
		let result = parse(input).unwrap();
		assert_eq!(result, expected);
	}

	#[test]
	fn parse_array() {
		let input = "(eq 0 (count (filter (gt 8.0) [1.0 2.0 10.0 20.0 30.0])))";

		let expected = func(
			"eq",
			vec![
				int(0).into_expr(),
				func(
					"count",
					vec![func(
						"filter",
						vec![
							func("gt", vec![float(8.0)]),
							array(vec![
								float(1.0),
								float(2.0),
								float(10.0),
								float(20.0),
								float(30.0),
							]),
						],
					)],
				),
			],
		);

		let result = parse(input).unwrap();
		assert_eq!(result, expected);
	}
}
