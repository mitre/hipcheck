// SPDX-License-Identifier: Apache-2.0

#![allow(unused)]

mod bridge;
mod env;
mod error;
pub mod expr;
mod json_pointer;
mod token;

use crate::policy_exprs::env::Env;
pub(crate) use crate::policy_exprs::{bridge::Tokens, expr::F64};
pub use crate::policy_exprs::{
	error::{Error, Result},
	expr::{Expr, Ident},
	token::LexingError,
};
use env::Binding;
pub use expr::{parse, Primitive};
use json_pointer::process_json_pointers;
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
		let processed_program = process_json_pointers(raw_program, context)?;
		let program = parse(&processed_program)?;
		let expr = eval(&self.env, &program)?;
		Ok(expr)
	}
}

/// Evaluate the `Expr`, returning a boolean.
pub(crate) fn eval(env: &Env, program: &Expr) -> Result<Expr> {
	let output = match program {
		Expr::Primitive(primitive) => Ok(Expr::Primitive(primitive.resolve(env)?)),
		Expr::Array(_) => Ok(program.clone()),
		Expr::Function(name, args) => {
			let binding = env
				.get(name)
				.ok_or_else(|| Error::UnknownFunction(name.deref().to_owned()))?;

			if let Binding::Fn(op) = binding {
				op(env, args)
			} else {
				Err(Error::FoundVarExpectedFunc(name.deref().to_owned()))
			}
		}
		Expr::Lambda(_, body) => Ok((**body).clone()),
		Expr::JsonPointer(_) => unreachable!(),
	};

	log::debug!("input: {program:?}, output: {output:?}");

	output
}

#[cfg(test)]
mod tests {
	use super::*;
	use test_log::test;

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
		assert_eq!(result, Expr::Primitive(Primitive::Bool(true)));
	}

	#[test]
	fn eval_foreach() {
		let program =
			"(eq 3 (count (filter (gt 8.0) (foreach (sub 1.0) [1.0 2.0 10.0 20.0 30.0]))))";
		let context = Value::Null;
		let result = Executor::std().parse_and_eval(program, &context).unwrap();
		assert_eq!(result, Expr::Primitive(Primitive::Bool(true)));
	}

	#[test]
	fn eval_basic_filter() {
		let program = "(filter (eq 0) [1 0 1 0 0 1 2])";
		let context = Value::Null;
		let result = Executor::std().parse_and_eval(program, &context).unwrap();
		assert_eq!(
			result,
			Expr::Array(vec![
				Primitive::Int(0),
				Primitive::Int(0),
				Primitive::Int(0)
			])
		);
	}
}
