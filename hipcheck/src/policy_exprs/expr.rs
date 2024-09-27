// SPDX-License-Identifier: Apache-2.0

use crate::policy_exprs::{
	env::{Binding, Env},
	token::Token,
	Error, Result, Tokens,
};
use itertools::Itertools;
use jiff::{Span, Zoned};
use nom::{
	branch::alt,
	combinator::{all_consuming, map},
	multi::many0,
	sequence::tuple,
	Finish as _, IResult,
};
use ordered_float::NotNan;
use std::{fmt::Display, ops::Deref};

#[cfg(test)]
use jiff::civil::Date;

/// A `deke` expression to evaluate.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Expr {
	/// Primitive data (ints, floats, bool).
	Primitive(Primitive),

	/// An array of primitive data.
	Array(Array),

	/// Stores the name of the function, followed by the args.
	Function(Function),

	/// Stores the name of the input variable, followed by the lambda body.
	Lambda(Lambda),

	/// Stores a late-binding for a JSON value.
	JsonPointer(JsonPointer),
}

/// An array of primitives.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Array {
	pub elts: Vec<Primitive>,
}
impl Array {
	pub fn new(elts: Vec<Primitive>) -> Self {
		Array { elts }
	}
}
impl From<Array> for Expr {
	fn from(value: Array) -> Self {
		Expr::Array(value)
	}
}

/// A `deke` function to evaluate.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Function {
	pub ident: Ident,
	pub args: Vec<Expr>,
}
impl Function {
	pub fn new(ident: Ident, args: Vec<Expr>) -> Self {
		Function { ident, args }
	}
}
impl From<Function> for Expr {
	fn from(value: Function) -> Self {
		Expr::Function(value)
	}
}

/// Stores the name of the input variable, followed by the lambda body.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Lambda {
	pub arg: Ident,
	pub body: Box<Expr>,
}
impl Lambda {
	pub fn new(arg: Ident, body: Box<Expr>) -> Self {
		Lambda { arg, body }
	}
}
impl From<Lambda> for Expr {
	fn from(value: Lambda) -> Self {
		Expr::Lambda(value)
	}
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

	/// Date-time value with timezone information using the [jiff] crate, which uses a modified version of ISO8601.
	/// This must include a date in the format <YYYY>-<MM>-<DD>.
	/// An optional time in the format T<HH>:[MM]:[SS] will be accepted after the date.
	/// Decimal fractions of hours and minutes are not allowed; use smaller time units instead (e.g. T10:30 instead of T10.5). Decimal fractions of seconds are allowed.
	/// The timezone is always set to UTC, but you can set an offeset from UTC by including +{HH}:[MM] or -{HH}:[MM]. The time will be adjusted to the correct UTC time during parsing.
	DateTime(Zoned),

	/// Span of time using the [jiff] crate, which uses a modified version of ISO8601.
	///
	/// Can include weeks, days, hours, minutes, and seconds (including decimal fractions of a second).
	/// While spans with months, years, or both are valid under IS08601 and supported by [jiff] in general, we do not allow them in Hipcheck policy expressions.
	/// This is because spans greater than a day require additional zoned datetime information in [jiff] (to determine e.g. how many days are in a year or month)
	/// before we can do time arithematic with them.
	/// We *do* allows spans with weeks, even though [jiff] has similar issues with those units.
	/// We take care of this by converting a week to a period of seven 24-hour days that [jiff] can handle in arithematic without zoned datetime information.
	///
	/// Spans are preceded by the letter "P" with any optional time units separated from optional date units by the letter "T".
	/// All units of dates and times are represented by single case-agnostic letter abbreviations after the number.
	/// For example, a span of one week, one day, one hour, one minute, and one-and-a-tenth seconds would be represented as
	/// "P1w1dT1h1m1.1s"
	Span(Span),
}
impl From<Primitive> for Expr {
	fn from(value: Primitive) -> Self {
		Expr::Primitive(value)
	}
}

/// A variable or function identifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ident(pub String);

/// A late-binding for a JSON pointer
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonPointer {
	pointer: String,
	value: Option<serde_json::Value>,
}
impl From<JsonPointer> for Expr {
	fn from(value: JsonPointer) -> Self {
		Expr::JsonPointer(value)
	}
}

/// A non-NaN 64-bit floating point number.
pub type F64 = NotNan<f64>;

impl Display for Expr {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Expr::Primitive(primitive) => write!(f, "{}", primitive),
			Expr::Array(array) => {
				write!(
					f,
					"[{}]",
					array.elts.iter().map(ToString::to_string).join(" ")
				)
			}
			Expr::Function(func) => {
				let args = func.args.iter().map(ToString::to_string).join(" ");
				write!(f, "({} {})", func.ident, args)
			}
			Expr::Lambda(l) => write!(f, "(lambda ({}) {}", l.arg, l.body),
			Expr::JsonPointer(pointer) => write!(f, "${}", pointer.pointer),
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
			Primitive::DateTime(dt) => write!(f, "{}", dt),
			Primitive::Span(span) => write!(f, "{}", span),
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
	fn parse_datetime(input) -> Result<Primitive>;
	pattern = Token::DateTime(dt) => Primitive::DateTime(*dt);
}

crate::data_variant_parser! {
	fn parse_span(input) -> Result<Primitive>;
	pattern = Token::Span(span) => Primitive::Span(*span);
}

crate::data_variant_parser! {
	fn parse_ident(input) -> Result<String>;
	pattern = Token::Ident(s) => s.to_owned();
}

crate::data_variant_parser! {
	fn parse_json_pointer(input) -> Result<Expr>;
	pattern = Token::JSONPointer(s) => Expr::JsonPointer(JsonPointer { pointer: s.to_owned(), value: None });
}

// Helper type for token parsing.
pub type Input<'source> = Tokens<'source, Token>;

/// Parse a single piece of primitive data.
fn parse_primitive(input: Input<'_>) -> IResult<Input<'_>, Primitive> {
	alt((
		parse_integer,
		parse_float,
		parse_bool,
		parse_datetime,
		parse_span,
	))(input)
}

/// Parse an array.
fn parse_array(input: Input<'_>) -> IResult<Input<'_>, Expr> {
	let parser = tuple((Token::OpenBrace, many0(parse_primitive), Token::CloseBrace));
	let mut parser = map(parser, |(_, inner, _)| Array::new(inner).into());
	parser(input)
}

/// Parse an expression.
fn parse_expr(input: Input<'_>) -> IResult<Input<'_>, Expr> {
	let primitive = map(parse_primitive, Expr::Primitive);
	alt((primitive, parse_array, parse_function, parse_json_pointer))(input)
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
		Function::new(Ident(ident), args).into()
	});
	parser(input)
}

pub fn parse(input: &str) -> Result<Expr> {
	let tokens = Tokens::new(input);
	let mut parser = all_consuming(parse_expr);

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
	use crate::policy_exprs::LexingError;
	use test_log::test;

	use jiff::{
		tz::{self, TimeZone},
		Span, Timestamp, Zoned,
	};

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
		Function::new(Ident(String::from(name)), args).into()
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

	fn datetime(val: Zoned) -> Primitive {
		Primitive::DateTime(val)
	}

	fn span(val: Span) -> Primitive {
		Primitive::Span(val)
	}

	fn array(vals: Vec<Primitive>) -> Expr {
		Array::new(vals).into()
	}

	fn json_ptr(name: &str) -> Expr {
		Expr::JsonPointer(JsonPointer {
			pointer: String::from(name),
			value: None,
		})
	}

	#[test]
	fn parse_bool() {
		let input = "#t";
		let expected = boolean(true).into_expr();
		let result = parse(input).unwrap();
		assert_eq!(result, expected);
	}

	#[test]
	fn parse_datetime() {
		let input = "2024-09-17T09:30:00-05";
		let result = parse(input).unwrap();

		let ts: Timestamp = input.parse().unwrap();
		let dt = Zoned::new(ts, TimeZone::UTC);
		let expected = datetime(dt).into_expr();

		assert_eq!(result, expected);
	}

	#[test]
	fn parse_simple_datetime() {
		let input = "2024-09-17";
		let result = parse(input).unwrap();

		let ts: Date = input.parse().unwrap();
		let dt = ts.to_zoned(TimeZone::UTC).unwrap();
		let expected = datetime(dt).into_expr();

		assert_eq!(result, expected);
	}

	#[test]
	fn parse_span() {
		let input = "P2W4DT1H30M";
		let result = parse(input).unwrap();

		let raw_span: Span = "P18DT1H30M".parse().unwrap();
		let expected = span(raw_span).into_expr();

		assert_eq!(result, expected);
	}

	#[test]
	fn parse_simple_span() {
		let input = "P2w";
		let result = parse(input).unwrap();

		let raw_span: Span = "P14d".parse().unwrap();
		let expected = span(raw_span).into_expr();

		assert_eq!(result, expected);
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

	#[test]
	fn parse_json_pointer_empty() {
		let input = "$";
		let expected = json_ptr("");
		let result = parse(input).unwrap();
		assert_eq!(result, expected);
	}

	#[test]
	fn parse_json_pointer_basic() {
		let input = "$/alpha";
		let expected = json_ptr("/alpha");
		let result = parse(input).unwrap();
		assert_eq!(result, expected);
	}
}
