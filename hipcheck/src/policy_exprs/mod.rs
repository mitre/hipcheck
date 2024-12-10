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
	expr::{
		Array, Expr, Function, Ident, JsonPointer, Lambda, PrimitiveType, ReturnableType, Type,
		Typed,
	},
	pass::{ExprMutator, ExprVisitor, FunctionResolver, TypeChecker, TypeFixer},
	token::LexingError,
};
use env::Binding;
pub use expr::{parse, Primitive};
use json_pointer::LookupJsonPointers;
use serde_json::Value;
use std::{ops::Deref, str::FromStr, sync::LazyLock};

static PASS_STD_FUNC_RES: LazyLock<FunctionResolver> = LazyLock::new(FunctionResolver::std);
static PASS_STD_TYPE_FIX: LazyLock<TypeFixer> = LazyLock::new(TypeFixer::std);
static PASS_STD_TYPE_CHK: LazyLock<TypeChecker> = LazyLock::new(TypeChecker::default);

pub fn std_pre_analysis_pipeline(mut expr: Expr) -> Result<Expr> {
	expr = PASS_STD_FUNC_RES.run(expr)?;
	expr = PASS_STD_TYPE_FIX.run(expr)?;
	PASS_STD_TYPE_CHK.run(&expr)?;
	Ok(expr)
}

pub fn std_post_analysis_pipeline(
	mut expr: Expr,
	context: Option<&Value>,
	run_pre_passes: bool,
) -> Result<Expr> {
	// Track whether we've done type checking or we've added something to require re-doing it
	let mut needs_check = true;
	if run_pre_passes {
		expr = std_pre_analysis_pipeline(expr)?;
		needs_check = false;
	}
	// Adding JSON context requires re-type checking
	if let Some(ctx) = context {
		expr = LookupJsonPointers::with_context(ctx).run(expr)?;
		needs_check = true;
	}
	if needs_check {
		PASS_STD_TYPE_CHK.run(&expr)?;
	}
	Env::std().run(expr)
}

pub fn std_parse(raw_program: &str) -> Result<Expr> {
	std_pre_analysis_pipeline(parse(raw_program)?)
}

pub fn std_exec(mut expr: Expr, context: Option<&Value>) -> Result<bool> {
	match std_post_analysis_pipeline(expr, context, false)? {
		Expr::Primitive(Primitive::Bool(b)) => Ok(b),
		result => Err(Error::DidNotReturnBool(result)),
	}
}

impl FromStr for Expr {
	type Err = crate::policy_exprs::error::Error;

	fn from_str(raw: &str) -> Result<Expr> {
		std_pre_analysis_pipeline(parse(raw)?)
	}
}

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
		let mut f = f;
		// first evaluate all the children
		f.args = f
			.args
			.into_iter()
			.map(|a| self.visit_expr(a))
			.collect::<Result<Vec<Expr>>>()?;
		let binding = self
			.get(&f.ident)
			.ok_or_else(|| Error::UnknownFunction(f.ident.deref().to_owned()))?;
		if let Binding::Fn(op_info) = binding {
			// Doesn't use `execute` because currently allows Functions that haven't been changed
			// to Lambdas
			(op_info.op)(self, &f.args)
		} else {
			Err(Error::FoundVarExpectedFunc(f.ident.deref().to_owned()))
		}
	}
	fn visit_lambda(&self, mut l: Lambda) -> Result<Expr> {
		// Eagerly evaluate the arguments to the lambda but not the func itself
		// Visit args, but ignore lambda ident because not yet bound
		l.body.args = l
			.body
			.args
			.drain(..)
			.map(|a| match a {
				Expr::Primitive(Primitive::Identifier(_)) => Ok(a),
				b => self.visit_expr(b),
			})
			.collect::<Result<Vec<Expr>>>()?;
		Ok(l.into())
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
		let expr = parse(program).unwrap();
		println!("EXPR: {:?}", &expr);
		let expr = FunctionResolver::std().run(expr).unwrap();
		let expr = TypeFixer::std().run(expr).unwrap();
		println!("RESOLVER RES: {:?}", expr);
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

	#[test]
	fn type_lambda() {
		let program = "(gt #t)";
		let expr = parse(program).unwrap();
		let expr = FunctionResolver::std().run(expr).unwrap();
		let expr = TypeFixer::std().run(expr).unwrap();
		let res_ty = TypeChecker::default().run(&expr);
		let Ok(Type::Lambda(l_ty)) = res_ty else {
			panic!();
		};
		let ret_ty = l_ty.get_return_type();
		assert_eq!(ret_ty, Ok(ReturnableType::Primitive(PrimitiveType::Bool)));
	}

	#[test]
	fn type_filter_bad_lambda_array() {
		// Should fail because can't compare ints and bools
		let program = "(filter (gt #t) [1 2])";
		let expr = parse(program).unwrap();
		let expr = FunctionResolver::std().run(expr).unwrap();
		let expr = TypeFixer::std().run(expr).unwrap();
		let res_ty = TypeChecker::default().run(&expr);
		assert!(matches!(
			res_ty,
			Err(Error::BadFuncArgType {
				idx: 0,
				got: Type::Primitive(PrimitiveType::Int),
				..
			})
		));
	}

	#[test]
	fn type_array_mixed_types() {
		// Should fail because array elts must have one primitive type
		let program = "(count [#t 2])";
		let mut expr = parse(program).unwrap();
		expr = FunctionResolver::std().run(expr).unwrap();
		let res_ty = TypeChecker::default().run(&expr);
		assert_eq!(
			res_ty,
			Err(Error::BadArrayElt {
				idx: 1,
				expected: PrimitiveType::Bool,
				got: PrimitiveType::Int
			})
		);
	}

	#[test]
	fn type_propagate_unknown() {
		// Type for array should be unknown because we can't know ident type
		let program = "(max [])";
		let mut expr = parse(program).unwrap();
		expr = FunctionResolver::std().run(expr).unwrap();
		let res_ty = TypeChecker::default().run(&expr);
		let Ok(Type::Function(f_ty)) = res_ty else {
			panic!()
		};
		assert_eq!(f_ty.get_return_type(), Ok(ReturnableType::Unknown));
	}

	#[test]
	fn type_not() {
		let program = "(not $)";
		let mut expr = parse(program).unwrap();
		expr = FunctionResolver::std().run(expr).unwrap();
		let res_ty = TypeChecker::default().run(&expr);
		println!("RESTY: {res_ty:?}");
		let Ok(Type::Function(f_ty)) = res_ty else {
			panic!()
		};
		let ret_ty = f_ty.get_return_type();
		assert_eq!(ret_ty, Ok(ReturnableType::Primitive(PrimitiveType::Bool)));
	}

	#[test]
	fn from_and_to_string() {
		let programs = vec!["(not $)", "(gt 0)", "(filter (gt 0) $/alpha)"];

		for program in programs {
			let mut expr = parse(program).unwrap();
			expr = FunctionResolver::std().run(expr).unwrap();
			expr = TypeFixer::std().run(expr).unwrap();
			let string = expr.to_string();
			assert_eq!(program, string);
		}
	}
}
