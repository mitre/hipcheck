// SPDX-License-Identifier: Apache-2.0

#![allow(unused)]

mod bridge;
mod env;
mod error;
pub mod expr;
mod json_pointer;
mod pass;
mod token;

use crate::policy_exprs::env::Env;
pub(crate) use crate::policy_exprs::{bridge::Tokens, expr::F64};
pub use crate::policy_exprs::{
	error::{Error, Result},
	expr::{Array, Expr, Function, Ident, JsonPointer, Lambda},
	pass::{ExprMutator, ExprVisitor},
	token::LexingError,
};
use env::Binding;
pub use expr::{parse, Primitive};
use json_pointer::LookupJsonPointers;
use serde_json::Value;
use std::ops::Deref;

/// Evaluates `deke` expressions.
pub struct Executor {
	env: Env<'static>,
}

impl Executor {
	/// Create an `Executor` with the standard set of functions defined.
	pub fn std() -> Self {
		Executor { env: Env::std() }
	}

	/// Run a `deke` program.
	pub fn run(&self, raw_program: &str, context: &Value) -> Result<bool> {
		match self.parse_and_eval(raw_program, context)? {
			Expr::Primitive(Primitive::Bool(b)) => Ok(b),
			result => Err(Error::DidNotReturnBool(result)),
		}
	}

	/// Run a `deke` program, but don't try to convert the result to a `bool`.
	pub fn parse_and_eval(&self, raw_program: &str, context: &Value) -> Result<Expr> {
		let program = parse(raw_program)?;
		// JSON Pointer lookup failures happen on this line.
		let processed_program = LookupJsonPointers::with_context(context).run(program)?;
		let expr = self.env.visit_expr(processed_program)?;
		Ok(expr)
	}
}
impl ExprMutator for Env<'_> {
	fn visit_primitive(&self, prim: Primitive) -> Result<Expr> {
		Ok(prim.resolve(self)?.into())
	}
	fn visit_function(&self, f: Function) -> Result<Expr> {
		let binding = self
			.get(&f.ident)
			.ok_or_else(|| Error::UnknownFunction(f.ident.deref().to_owned()))?;
		if let Binding::Fn(op) = binding {
			(op)(self, &f.args)
		} else {
			Err(Error::FoundVarExpectedFunc(f.ident.deref().to_owned()))
		}
	}
	fn visit_lambda(&self, l: Lambda) -> Result<Expr> {
		Ok((*l.body).clone())
	}
	fn visit_json_pointer(&self, jp: JsonPointer) -> Result<Expr> {
		let expr = &jp.value;
		match expr {
			None => Err(Error::InternalError(format!(
				"JsonPointer's `value` field was not set. \
				All `value` fields must be set by `LookupJsonPointers` before evaluation. \
				JsonPointer: {:?}",
				&jp
			))),
			Some(expr) => Ok(*expr.to_owned()),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use test_log::test;

	#[test]
	fn visitor_replaces_json_pointer() {
		// Assume that json_pointer::LookupJsonPointers has already run,
		// so `value` will contain an Expr.
		let expr = Expr::JsonPointer(JsonPointer {
			pointer: "".to_owned(),
			value: Some(Box::new(Primitive::Bool(true).into())),
		});
		let expected = Primitive::Bool(true).into();

		let result = Env::std().visit_expr(expr);
		assert_eq!(result, Ok(expected))
	}

	#[test]
	fn run_bool() {
		let program = "#t";
		let context = Value::Null;
		let is_true = Executor::std().run(program, &context).unwrap();
		assert!(is_true);
	}

	#[test]
	fn run_jsonptr_bool() {
		let program = "$";
		let context = Value::Bool(true);
		let is_true = Executor::std().run(program, &context).unwrap();
		assert!(is_true);
	}

	#[test]
	fn run_basic() {
		let program = "(eq (add 1 2) 3)";
		let context = Value::Null;
		let is_true = Executor::std().run(program, &context).unwrap();
		assert!(is_true);
	}

	#[test]
	fn eval_basic() {
		let program = "(add 1 2)";
		let context = Value::Null;
		let result = Executor::std().parse_and_eval(program, &context).unwrap();
		assert_eq!(result, Expr::Primitive(Primitive::Int(3)));
	}

	#[test]
	fn eval_divz_int_zero() {
		let program = "(divz 1 0)";
		let context = Value::Null;
		let result = Executor::std().parse_and_eval(program, &context).unwrap();
		assert_eq!(
			result,
			Expr::Primitive(Primitive::Float(F64::new(0.0).unwrap()))
		);
	}

	#[test]
	fn eval_divz_int() {
		let program = "(divz 1 2)";
		let context = Value::Null;
		let result = Executor::std().parse_and_eval(program, &context).unwrap();
		assert_eq!(
			result,
			Expr::Primitive(Primitive::Float(F64::new(0.5).unwrap()))
		);
	}

	#[test]
	fn eval_divz_float() {
		let program = "(divz 1.0 2.0)";
		let context = Value::Null;
		let result = Executor::std().parse_and_eval(program, &context).unwrap();
		assert_eq!(
			result,
			Expr::Primitive(Primitive::Float(F64::new(0.5).unwrap()))
		);
	}

	#[test]
	fn eval_divz_float_zero() {
		let program = "(divz 1.0 0.0)";
		let context = Value::Null;
		let result = Executor::std().parse_and_eval(program, &context).unwrap();
		assert_eq!(
			result,
			Expr::Primitive(Primitive::Float(F64::new(0.0).unwrap()))
		);
	}

	#[test]
	fn eval_bools() {
		let program = "(neq 1 2)";
		let context = Value::Null;
		let result = Executor::std().parse_and_eval(program, &context).unwrap();
		assert_eq!(result, Expr::Primitive(Primitive::Bool(true)));
	}

	#[test]
	fn eval_array() {
		let program = "(max [1 4 6 10 2 3 0])";
		let context = Value::Null;
		let result = Executor::std().parse_and_eval(program, &context).unwrap();
		assert_eq!(result, Expr::Primitive(Primitive::Int(10)));
	}

	#[test]
	fn run_array() {
		let program = "(eq 7 (count [1 4 6 10 2 3 0]))";
		let context = Value::Null;
		let is_true = Executor::std().run(program, &context).unwrap();
		assert!(is_true);
	}

	#[test]
	fn eval_higher_order_func() {
		let program = "(eq 3 (count (filter (gt 8.0) [1.0 2.0 10.0 20.0 30.0])))";
		let context = Value::Null;
		let result = Executor::std().parse_and_eval(program, &context).unwrap();
		assert_eq!(result, Primitive::Bool(true).into());
	}

	#[test]
	fn eval_foreach() {
		let program =
			"(eq 3 (count (filter (gt 8.0) (foreach (sub 1.0) [1.0 2.0 10.0 20.0 30.0]))))";
		let context = Value::Null;
		let result = Executor::std().parse_and_eval(program, &context).unwrap();
		assert_eq!(result, Primitive::Bool(true).into());
	}

	#[test]
	fn eval_basic_filter() {
		let program = "(filter (eq 0) [1 0 1 0 0 1 2])";
		let context = Value::Null;
		let result = Executor::std().parse_and_eval(program, &context).unwrap();
		assert_eq!(
			result,
			Array::new(vec![
				Primitive::Int(0),
				Primitive::Int(0),
				Primitive::Int(0)
			])
			.into()
		);
	}

	#[test]
	fn eval_upcasted_int() {
		let program_and_expected = vec![
			("(lte 3 3.0)", Expr::Primitive(Primitive::Bool(true))),
			(
				"(add 3 5.5)",
				Expr::Primitive(Primitive::Float(F64::new(8.5).unwrap())),
			),
		];
		let context = Value::Null;
		for (program, expected) in program_and_expected.into_iter() {
			let result = Executor::std().parse_and_eval(program, &context).unwrap();
			assert_eq!(result, expected);
		}
	}

	#[test]
	fn eval_datetime_span_add() {
		let date = "2024-09-26";
		let span = "P1w";
		let eval_fmt = "(add {} {})";
		let context = Value::Null;
		let expected = parse("2024-10-03").unwrap();
		let result1 = Executor::std()
			.parse_and_eval(format!("(add {} {})", date, span).as_str(), &context)
			.unwrap();
		assert_eq!(expected, result1);
		let result2 = Executor::std()
			.parse_and_eval(format!("(add {} {})", span, date).as_str(), &context)
			.unwrap();
		assert_eq!(expected, result2);
	}
}
