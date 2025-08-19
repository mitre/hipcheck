// SPDX-License-Identifier: Apache-2.0

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
	pass::{ExprMutator, ExprVisitor, FunctionResolver, TypeChecker, TypeFixer},
	token::LexingError,
};
use env::Binding;
use expr::FunctionDef;
pub use expr::{Primitive, parse};
use jiff::{
	SpanRound, Unit,
	fmt::friendly::{Designator, Spacing, SpanPrinter},
};
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

pub fn std_exec(expr: Expr, context: Option<&Value>) -> Result<bool> {
	match std_post_analysis_pipeline(expr, context, false)? {
		Expr::Primitive(Primitive::Bool(b)) => Ok(b),
		result => Err(Error::DidNotReturnBool(Box::new(result))),
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

	#[cfg(test)]
	/// Run a `deke` program.
	pub fn run(&self, raw_program: &str, context: &Value) -> Result<bool> {
		match self.parse_and_eval(raw_program, context)? {
			Expr::Primitive(Primitive::Bool(b)) => Ok(b),
			result => Err(Error::DidNotReturnBool(Box::new(result))),
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
			.ok_or_else(|| Error::UnknownFunction(f.ident.deref().to_owned().into_boxed_str()))?;
		if let Binding::Fn(op_info) = binding {
			// Doesn't use `execute` because currently allows Functions that haven't been changed
			// to Lambdas
			(op_info.op)(self, &f.args)
		} else {
			Err(Error::FoundVarExpectedFunc(
				f.ident.deref().to_owned().into_boxed_str(),
			))
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
			None => Err(Error::InternalError(
				format!(
					"JsonPointer's `value` field was not set. \
				All `value` fields must be set by `LookupJsonPointers` before evaluation. \
				JsonPointer: {:?}",
					&jp
				)
				.into_boxed_str(),
			)),
			Some(expr) => Ok(*expr.to_owned()),
		}
	}
}

/// Return an English language explanation for a plugin's analysis **and** the calculated value used in the policy expression.
///
/// If the analysis succeded, return what it was required to see and what it saw.
/// If the analysis failed, return what it expected to see and what it saw instead.
pub fn parse_expr_to_english(
	input: &Expr,
	message: &str,
	value: &Option<Value>,
	passed: bool,
) -> Result<(String, Option<String>)> {
	// Create a standard environment, with its list of functions and their English descriptions
	let env = Env::std();
	// Store that environment and the plugin explanation message in a struct for English parsing
	let english = English {
		env: env.clone(),
		message: message.to_string(),
	};

	// Check that the "top level" of the policy expression is a function, then recursively parse that function into an English language description of why the plugin failed
	if let Expr::Function(original_func) = input {
		// Get the function's args
		let args = &original_func.args;

		// Confirm that the outermost function has two arguments
		if args.len() != 2 {
			return Err(Error::MissingArgs);
		}

		// See which of the function's arguments is a primitive (i.e. the top level expected value)
		// If this is the first argument, swap the order of the arguments before parsing the function to English
		let func = match (&args[0], &args[1]) {
			(&Expr::Primitive(_), _) => &original_func.swap_args(),
			(_, &Expr::Primitive(_)) => original_func,
			_ => return Err(Error::MissingArgs),
		};

		// Get whichever of the function's arguments is **not** a primitive for evaluation
		let inner = &func.args[0];

		// Evaluate that argument using the value returned by the plugin to see what the top level operator is comparing the expected value to
		let inner_value = match value {
			Some(context) => match Executor::std().parse_and_eval(&inner.to_string(), context)? {
				Expr::Primitive(prim) => Some(english.visit_primitive(&prim)?),
				_ => return Err(Error::BadReturnType(Box::new(inner.clone()))),
			},
			None => None,
		};

		// Format the returned String depending on whether the plugin's analysis succeded or not
		if passed {
			// Recursively parse the argument that is not a primitive to English
			let english_inner = english.visit_expr(inner)?;

			// Parse the top level operator to English
			let function_def = get_function_def(func, &env)?;
			let operator = parse_function_operator(&function_def);

			// Parse the top level primitive to English
			let threshold = english.visit_expr(&func.args[1])?;

			return Ok((
				format!(
					"{} was {english_inner}, which was required {operator} {threshold}",
					inner_value
						.clone()
						.unwrap_or("no value was returned by the query".to_string())
				),
				inner_value,
			));
		}

		// Recursively parse the top level function to English
		let english_expr = english.visit_function(func)?;
		return Ok((
			format!(
				"Expected {english_expr} but it was {}",
				inner_value
					.clone()
					.unwrap_or("no value was returned by the query".to_string())
			),
			inner_value,
		));
	}

	Err(Error::MissingIdent)
}

/// Struct that contains a basic environment, with its English function descriptions, and a plugin explanation message.
pub struct English<'a> {
	env: Env<'a>,
	message: String,
}

impl English<'_> {
	/// Returns compact English phrases for common policy expressions indicating a filtered count
	/// These should look like `(count (filter (<operator> <primitive>) $))`
	fn parse_count_function(&self, func: &Function) -> Option<String> {
		// The function must have a count operator
		if func.ident.to_string() != "count" {
			return None;
		}

		// The first argument must be another function
		let Expr::Function(inner_func) = &func.args[0] else {
			return None;
		};

		// That inner function must have a filter operator
		// Include an extra safety check that the function has a second argument
		if inner_func.ident.to_string() != "filter" || inner_func.args.len() != 2 {
			return None;
		}

		// The inner filter function must have a lambda and a JSON pointer as its arguments
		let (Expr::Lambda(lambda), Expr::JsonPointer(_)) =
			(&inner_func.args[0], &inner_func.args[1])
		else {
			return None;
		};

		let lambda_func = &lambda.body;
		let lambda_arg = &lambda_func.args[1];

		// Special case of `(count (filter (eq #t) $))`
		if lambda_func.ident.to_string() == "eq"
			&& let Expr::Primitive(Primitive::Bool(bool)) = lambda_arg
			&& *bool
		{
			return Some(format!("the number of {}", self.message.clone()));
		}

		// More general lambda expressions
		let english_operator = get_function_def(lambda_func, &self.env).ok()?.english;
		let english_expression = self.visit_expr(lambda_arg).ok()?;

		Some(format!(
			"the number of {} {} {}",
			self.message.clone(),
			english_operator,
			english_expression
		))
	}

	/// Returns compact English phrases for common policy expressions indicating a filtered percent calculation
	/// These should look like `(divz (count (filter (<operator> <primitive>) $)) (count $))`
	fn parse_percent_function(&self, func: &Function) -> Option<String> {
		// The function must have a division operator
		// Include an extra safety check that the function has a second argument
		if func.ident.to_string() != "divz" || func.args.len() != 2 {
			return None;
		}

		// The arguments of the function must both be other functions
		let (Expr::Function(num_func), Expr::Function(denom_func)) = (&func.args[0], &func.args[1])
		else {
			return None;
		};

		// Both of those nested functions must have count operators
		if num_func.ident.to_string() != "count" || denom_func.ident.to_string() != "count" {
			return None;
		}

		// The first count function must have another function as its argument
		// The second count function must have a JSON pointer as its argument
		let (Expr::Function(inner_func), Expr::JsonPointer(_)) =
			(&num_func.args[0], &denom_func.args[0])
		else {
			return None;
		};

		// The inner function of the first count funciton must have a filter operator
		// Include an extra safety check that the function has a second argument
		if inner_func.ident.to_string() != "filter" || inner_func.args.len() != 2 {
			return None;
		}
		// The inner filter function must have a lamda and a JSON pointer as its arguments
		let (Expr::Lambda(lambda), Expr::JsonPointer(_)) =
			(&inner_func.args[0], &inner_func.args[1])
		else {
			return None;
		};

		let lambda_func = &lambda.body;
		let lambda_arg = &lambda_func.args[1];

		// Special case of `(divz (count (filter (eq #t) $)) (count $))`
		if lambda_func.ident.to_string() == "eq"
			&& let Expr::Primitive(Primitive::Bool(bool)) = lambda_arg
			&& *bool
		{
			return Some(format!("the percent of {}", self.message.clone()));
		}

		// More general lambda expressions
		let english_operator = get_function_def(lambda_func, &self.env).ok()?.english;
		let english_expression = self.visit_expr(lambda_arg).ok()?;

		Some(format!(
			"the percent of {} {} {}",
			self.message.clone(),
			english_operator,
			english_expression
		))
	}
}

// Trait implementation to return English descriptions of an Expr
impl ExprVisitor<Result<String>> for English<'_> {
	/// Parse a function expression into an English string
	fn visit_function(&self, func: &Function) -> Result<String> {
		let env = &self.env;

		// Check for special cases
		match func.ident.to_string().as_ref() {
			// Check if the function is a filtered count function, and return a simple English string if it is
			"count" => {
				if let Some(count_string) = self.parse_count_function(func) {
					return Ok(count_string);
				}
			}
			// Check if the function is a percent function, and return a simple English string if it is
			"divz" => {
				if let Some(percent_string) = self.parse_percent_function(func) {
					return Ok(percent_string);
				}
			}
			_ => {}
		}

		// Parse the function operator to English
		let function_def = get_function_def(func, env)?;
		let operator = parse_function_operator(&function_def);

		// Get the number of args the function should have
		let expected_args = function_def.expected_args;

		// Get the function's args
		let args = &func.args;

		// Check for an invalid number of arguments
		if args.len() < expected_args {
			return Err(Error::NotEnoughArgs {
				name: func.ident.to_string().into_boxed_str(),
				expected: expected_args,
				given: args.len(),
			});
		}
		if args.len() > expected_args {
			return Err(Error::TooManyArgs {
				name: func.ident.to_string().into_boxed_str(),
				expected: expected_args,
				given: args.len(),
			});
		}

		if args.len() == 2 {
			// If there are two arguments, parse a function comparing a pair of some combination of primitives,
			// JSON pointers, nested functions (including lambdas in the first position), or arrays (in the second position) to English
			if matches!(args[0], Expr::Array(_)) || matches!(args[1], Expr::Lambda(_)) {
				return Err(Error::BadType("English::visit_function()"));
			}
			let argument_1 = self.visit_expr(&args[0])?;
			let argument_2 = self.visit_expr(&args[1])?;

			Ok(format!("{} {} {}", argument_1, operator, argument_2))
		} else {
			// If there is one argument, parse a function operating on an array, JSON pointer, or a nested function to English
			if matches!(args[0], Expr::Lambda(_)) {
				return Err(Error::BadType("English::visit_function()"));
			}
			let argument = self.visit_expr(&args[0])?;

			Ok(format!("{} {}", operator, argument))
		}
	}

	/// Parse a lambda expression into an English string
	fn visit_lambda(&self, func: &Lambda) -> Result<String> {
		let env = &self.env;

		// Get the lambda function from the lambda
		let function = &func.body;

		// Parse the lambda's function operator to English
		let function_def = get_function_def(function, env)?;
		let operator = parse_function_operator(&function_def);

		// Get the lambda function's argument and parse it to English
		// Note: The useful arugment for a lambda function is the *second* argument
		let args = &function.args;
		let argument = self.visit_expr(&args[1])?;

		Ok(format!("\"{} {}\"", operator, argument))
	}

	// Parse a primitive type expression to English
	fn visit_primitive(&self, prim: &Primitive) -> Result<String> {
		match prim {
			Primitive::Bool(true) => Ok("true".to_string()),
			Primitive::Bool(false) => Ok("false".to_string()),
			Primitive::Int(i) => Ok(i.to_string()),
			Primitive::Float(f) => Ok(f.to_string()),
			Primitive::DateTime(dt) => Ok(dt.to_string()),
			Primitive::Span(span) => {
				let printer = SpanPrinter::new()
					.spacing(Spacing::BetweenUnitsAndDesignators)
					.designator(Designator::Verbose);
				Ok(printer
					.span_to_string(&span.round(SpanRound::new().smallest(Unit::Day)).unwrap()))
			}
			_ => Err(Error::BadType("English::visit_primitive()")),
		}
	}

	// Parse a primitive type array expression to English
	fn visit_array(&self, arr: &Array) -> Result<String> {
		let max_length = 5;

		let elts = &arr.elts;

		let english_elts = match elts.len() > max_length {
			false => elts
				.iter()
				.map(|p| self.visit_primitive(p).unwrap())
				.collect::<Vec<String>>()
				.join(","),
			true => {
				let mut english_elts = elts[..max_length]
					.iter()
					.map(|p| self.visit_primitive(p).unwrap())
					.collect::<Vec<String>>()
					.join(",");
				english_elts.push_str("...");
				english_elts
			}
		};
		Ok(format!("the array [{}]", english_elts))
	}

	// Parse a JSON pointer expression into English by returning the explanation message for the plugin
	fn visit_json_pointer(&self, _func: &JsonPointer) -> Result<String> {
		Ok(self.message.clone())
	}
}

// Get a function's definition from the environment
fn get_function_def(func: &Function, env: &Env) -> Result<FunctionDef> {
	// Get the function operator from the list of functions in the environment
	let ident = &func.ident;
	let fn_name = ident.to_string();

	let function_def = match env.get(&fn_name) {
		Some(binding) => match binding {
			Binding::Fn(function_def) => function_def,
			_ => {
				return Err(Error::UnknownFunction(
					format!("Given function name {} is not a function", fn_name).into_boxed_str(),
				));
			}
		},
		_ => {
			return Err(Error::UnknownFunction(
				format!(
					"Given function name {} not found in list of functions",
					fn_name
				)
				.into_boxed_str(),
			));
		}
	};

	Ok(function_def)
}

// Parse a function's operator to English from its function definition
fn parse_function_operator(function_def: &FunctionDef) -> String {
	// Convert the operator to English, with additional phrasing specific to comparison operators in a function
	match function_def.name.as_ref() {
		"gt" | "lt" | "gte" | "lte" | "eq" | "ne" => format!("to be {}", function_def.english),
		_ => function_def.english.clone(),
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::policy_exprs::expr::{PrimitiveType, ReturnableType, Type};
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
		assert_eq!(
			res_ty,
			Err(Error::BadFuncArgType {
				name: "gt".to_owned().into_boxed_str(),
				idx: 0,
				expected: "a float, int, bool, span, or datetime"
					.to_owned()
					.into_boxed_str(),
				got: Box::new(Type::Primitive(PrimitiveType::Int))
			})
		)
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
