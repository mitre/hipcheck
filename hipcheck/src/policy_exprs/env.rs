// SPDX-License-Identifier: Apache-2.0

use crate::policy_exprs::{
	expr::{
		ArrayType as ExprArrayType, Function, FunctionDef, FunctionType, Op, PrimitiveType,
		ReturnableType, Type, TypeChecker, Typed,
	},
	pass::ExprMutator,
	Array as StructArray, Error, Expr, Function as StructFunction, Ident, Lambda as StructLambda,
	Primitive, Result, F64,
};
use itertools::Itertools as _;
use jiff::{Span, Zoned};
use std::{cmp::Ordering, collections::HashMap, ops::Not as _};
use Expr::*;
use Primitive::*;

/// Environment, containing bindings of names to functions and variables.
pub struct Env<'parent> {
	/// Map of bindings,.
	bindings: HashMap<String, Binding>,

	/// Possible pointer to parent, for lexical scope.
	parent: Option<&'parent Env<'parent>>,
}

/// A binding in the environment.
#[derive(Clone)]
pub enum Binding {
	/// A function.
	Fn(FunctionDef),

	/// A primitive value.
	Var(Primitive),
}

// Ensure that type of array elements is valid with a lambda
fn ty_check_higher_order_lambda(
	l_ty: &FunctionType,
	arr_ty: &ExprArrayType,
) -> Result<ReturnableType> {
	if let Some(arr_elt_ty) = arr_ty {
		// Copy the lambda function type, replace ident with arr_elt_ty
		let mut try_l_ty = l_ty.clone();
		let first_arg = try_l_ty.arg_tys.get_mut(0).ok_or(Error::NotEnoughArgs {
			name: "".to_owned(),
			expected: 1,
			given: 0,
		})?;
		*first_arg = Type::Primitive(*arr_elt_ty);
		// If this returns error, means array type was incorrect for lambda
		try_l_ty.get_return_type()
	} else {
		Ok(ReturnableType::Unknown)
	}
}

// Expects args to contain [lambda, array]
fn ty_filter(args: &[Type]) -> Result<ReturnableType> {
	let arr_ty = expect_array_at(args, 1)?;

	let wrapped_l_ty = args.first().ok_or(Error::InternalError(
		"we were supposed to have already checked that there are at least two arguments".to_owned(),
	))?;
	let Type::Lambda(l_ty) = wrapped_l_ty else {
		return Err(Error::BadFuncArgType {
			name: "".to_owned(),
			idx: 0,
			expected: "a lambda".to_owned(),
			got: wrapped_l_ty.clone(),
		});
	};

	let res_ty = ty_check_higher_order_lambda(l_ty, &arr_ty)?;
	match res_ty {
		ReturnableType::Primitive(PrimitiveType::Bool) | ReturnableType::Unknown => {
			Ok(ReturnableType::Array(arr_ty))
		}
		_ => Err(Error::BadFuncArgType {
			name: "".to_owned(),
			idx: 0,
			expected: "a bool-returning lambda".to_owned(),
			got: Type::Lambda(l_ty.clone()),
		}),
	}
}

// Expects args to contain [lambda, array]
fn ty_higher_order_bool_fn(args: &[Type]) -> Result<ReturnableType> {
	let arr_ty = expect_array_at(args, 1)?;

	let wrapped_l_ty = args.first().ok_or(Error::InternalError(
		"we were supposed to have already checked that there are at least two arguments".to_owned(),
	))?;
	let Type::Lambda(l_ty) = wrapped_l_ty else {
		return Err(Error::BadFuncArgType {
			name: "".to_owned(),
			idx: 0,
			expected: "a lambda".to_owned(),
			got: wrapped_l_ty.clone(),
		});
	};

	let res_ty = ty_check_higher_order_lambda(l_ty, &arr_ty)?;
	match res_ty {
		ReturnableType::Primitive(PrimitiveType::Bool) | ReturnableType::Unknown => {
			Ok(ReturnableType::Primitive(PrimitiveType::Bool))
		}
		_ => Err(Error::BadFuncArgType {
			name: "".to_owned(),
			idx: 0,
			expected: "a bool-returning lambda".to_owned(),
			got: Type::Lambda(l_ty.clone()),
		}),
	}
}

// Type of dynamic function is dependent on first arg
fn ty_inherit_first(args: &[Type]) -> Result<ReturnableType> {
	args.first().ok_or(
        Error::InternalError("type checking function expects one argument, was incorrectly applied to a function that takes none".to_owned())
    )?.try_into()
}

fn ty_from_first_arr(args: &[Type]) -> Result<ReturnableType> {
	let arr_ty = expect_array_at(args, 0)?;
	Ok(match arr_ty {
		None => ReturnableType::Unknown,
		Some(p_ty) => ReturnableType::Primitive(p_ty),
	})
}

fn expect_primitive_at(args: &[Type], idx: usize) -> Result<Option<PrimitiveType>> {
	let arg = args
		.get(idx)
		.ok_or(Error::InternalError(
			"we were supposed to have already checked that function had enough arguments"
				.to_owned(),
		))?
		.try_into()?;

	match arg {
		ReturnableType::Primitive(p) => Ok(Some(p)),
		ReturnableType::Array(a) => Err(Error::BadFuncArgType {
			name: "".to_owned(),
			idx,
			expected: "a primitive type".to_owned(),
			got: Type::Array(a),
		}),
		ReturnableType::Unknown => Ok(None),
	}
}

fn expect_array_at(args: &[Type], idx: usize) -> Result<ExprArrayType> {
	let arg = args
		.get(idx)
		.ok_or(Error::InternalError(
			"we were supposed to have already checked that function had enough arguments"
				.to_owned(),
		))?
		.try_into()?;

	match arg {
		ReturnableType::Array(a) => Ok(a),
		ReturnableType::Unknown => Ok(None),
		ReturnableType::Primitive(p) => Err(Error::BadFuncArgType {
			name: "".to_owned(),
			idx,
			expected: "an array".to_owned(),
			got: Type::Primitive(p),
		}),
	}
}

fn ty_divz(args: &[Type]) -> Result<ReturnableType> {
	let opt_ty_1 = expect_primitive_at(args, 0)?;
	let opt_ty_2 = expect_primitive_at(args, 1)?;
	use PrimitiveType::*;

	let (bad, idx) = match (opt_ty_1, opt_ty_2) {
		(None | Some(Int | Float), None | Some(Int | Float)) => return Ok(Float.into()),
		(Some(x), None | Some(_)) => (x, 0),
		(None, Some(x)) => (x, 1),
	};

	Err(Error::BadFuncArgType {
		name: "".to_owned(),
		idx,
		expected: "an int or float".to_owned(),
		got: Type::Primitive(bad),
	})
}

fn ty_arithmetic_binary_ops(args: &[Type]) -> Result<ReturnableType> {
	// ensure both ops result in primitive types or unknown
	let opt_ty_1 = expect_primitive_at(args, 0)?;
	let opt_ty_2 = expect_primitive_at(args, 1)?;
	use PrimitiveType::*;
	use ReturnableType::*;

	let (bad, idx) = match (opt_ty_1, opt_ty_2) {
		(None, None) => return Ok(Unknown),
		(None | Some(Int), None | Some(Int)) => return Ok(Primitive(Int)),
		(None | Some(Int | Float), None | Some(Int | Float)) => return Ok(Primitive(Float)),
		(None, Some(Span)) => return Ok(Unknown),
		(Some(Span), None | Some(Span)) => return Ok(Primitive(DateTime)),
		(Some(DateTime), None | Some(Span)) => return Ok(Primitive(DateTime)),
		(Some(x), _) => (x, 0),
		(_, Some(x)) => (x, 1),
	};

	Err(Error::BadFuncArgType {
		name: "".to_owned(),
		idx,
		expected: "a float, int, span, or datetime".to_owned(),
		got: Type::Primitive(bad),
	})
}

fn ty_foreach(args: &[Type]) -> Result<ReturnableType> {
	expect_array_at(args, 1)?;
	let first_arg = args.first().ok_or(Error::InternalError(
		"we were supposed to have already checked that there are at least two arguments".to_owned(),
	))?;
	let fty = match first_arg {
		Type::Lambda(f) => f,
		other => {
			return Err(Error::BadFuncArgType {
				name: "foreach".to_owned(),
				idx: 0,
				expected: "lambda".to_owned(),
				got: other.clone(),
			});
		}
	};
	fty.get_return_type()
}

fn ty_comp(args: &[Type]) -> Result<ReturnableType> {
	let resp = Ok(Bool.into());
	let opt_ty_1: Option<PrimitiveType> = expect_primitive_at(args, 0)?;
	let opt_ty_2: Option<PrimitiveType> = expect_primitive_at(args, 1)?;
	use PrimitiveType::*;
	use ReturnableType::*;
	let (bad, idx) = match (opt_ty_1, opt_ty_2) {
		(None, None) => return Ok(Primitive(Bool)),
		(None | Some(Int), None | Some(Int)) => return resp,
		(None | Some(Int | Float), None | Some(Int | Float)) => return resp,
		(None | Some(Span), None | Some(Span)) => return resp,
		(None | Some(Bool), None | Some(Bool)) => return resp,
		(None | Some(DateTime), None | Some(DateTime)) => return resp,
		(Some(x), _) => (x, 0),
		(_, Some(x)) => (x, 1),
	};
	Err(Error::BadFuncArgType {
		name: "".to_owned(),
		idx,
		expected: "a float, int, bool, span, or datetime".to_owned(),
		got: Type::Primitive(bad),
	})
}

fn ty_count(_args: &[Type]) -> Result<ReturnableType> {
	Ok(PrimitiveType::Int.into())
}

fn ty_avg(args: &[Type]) -> Result<ReturnableType> {
	use PrimitiveType::*;
	let arr_ty = expect_array_at(args, 0)?;
	match arr_ty {
		None | Some(Int) | Some(Float) => Ok(Float.into()),
		Some(x) => Err(Error::BadFuncArgType {
			name: "".to_owned(),
			idx: 0,
			expected: "array of ints or floats".to_owned(),
			got: Type::Array(Some(x)),
		}),
	}
}

fn ty_duration(args: &[Type]) -> Result<ReturnableType> {
	use PrimitiveType::*;
	let opt_ty_1 = expect_primitive_at(args, 0)?;
	let opt_ty_2 = expect_primitive_at(args, 1)?;
	match opt_ty_1 {
		None | Some(DateTime) => (),
		Some(got) => {
			return Err(Error::BadFuncArgType {
				name: "".to_owned(),
				idx: 0,
				expected: "a datetime".to_owned(),
				got: Type::Primitive(got),
			});
		}
	}
	match opt_ty_2 {
		None | Some(DateTime) => (),
		Some(got) => {
			return Err(Error::BadFuncArgType {
				name: "".to_owned(),
				idx: 1,
				expected: "a datetime".to_owned(),
				got: Type::Primitive(got),
			});
		}
	}
	Ok(PrimitiveType::Span.into())
}

fn ty_bool_unary(args: &[Type]) -> Result<ReturnableType> {
	use PrimitiveType::*;
	match expect_primitive_at(args, 0)? {
		None | Some(Bool) => Ok(PrimitiveType::Bool.into()),
		Some(got) => Err(Error::BadFuncArgType {
			name: "".to_owned(),
			idx: 0,
			expected: "a bool".to_owned(),
			got: Type::Primitive(got),
		}),
	}
}

fn ty_bool_binary(args: &[Type]) -> Result<ReturnableType> {
	use PrimitiveType::*;
	let opt_ty_1 = expect_primitive_at(args, 0)?;
	let opt_ty_2 = expect_primitive_at(args, 1)?;
	match opt_ty_1 {
		None | Some(Bool) => (),
		Some(got) => {
			return Err(Error::BadFuncArgType {
				name: "".to_owned(),
				idx: 0,
				expected: "a bool".to_owned(),
				got: Type::Primitive(got),
			});
		}
	}
	match opt_ty_2 {
		None | Some(Bool) => (),
		Some(got) => {
			return Err(Error::BadFuncArgType {
				name: "".to_owned(),
				idx: 1,
				expected: "a bool".to_owned(),
				got: Type::Primitive(got),
			});
		}
	}
	Ok(PrimitiveType::Bool.into())
}

impl Env<'_> {
	/// Create an empty environment.
	fn empty() -> Self {
		Env {
			bindings: HashMap::new(),
			parent: None,
		}
	}

	/// Create the standard environment.
	pub fn std() -> Self {
		let mut env = Env::empty();

		// Comparison functions.
		env.add_fn("gt", gt, 2, ty_comp);
		env.add_fn("lt", lt, 2, ty_comp);
		env.add_fn("gte", gte, 2, ty_comp);
		env.add_fn("lte", lte, 2, ty_comp);
		env.add_fn("eq", eq, 2, ty_comp);
		env.add_fn("neq", neq, 2, ty_comp);

		// Math functions.
		env.add_fn("add", add, 2, ty_arithmetic_binary_ops);
		env.add_fn("sub", sub, 2, ty_arithmetic_binary_ops);
		env.add_fn("divz", divz, 2, ty_divz);

		// Additional datetime math functions
		env.add_fn("duration", duration, 2, ty_duration);

		// Logical functions.
		env.add_fn("and", and, 2, ty_bool_binary);
		env.add_fn("or", or, 2, ty_bool_binary);
		env.add_fn("not", not, 1, ty_bool_unary);

		// Array math functions.
		env.add_fn("max", max, 1, ty_from_first_arr);
		env.add_fn("min", min, 1, ty_from_first_arr);
		env.add_fn("avg", avg, 1, ty_avg);
		env.add_fn("median", median, 1, ty_from_first_arr);
		env.add_fn("count", count, 1, ty_count);

		// Array logic functions.
		env.add_fn("all", all, 1, ty_higher_order_bool_fn);
		env.add_fn("nall", nall, 1, ty_higher_order_bool_fn);
		env.add_fn("some", some, 1, ty_higher_order_bool_fn);
		env.add_fn("none", none, 1, ty_higher_order_bool_fn);

		// Array higher-order functions.
		env.add_fn("filter", filter, 2, ty_filter);
		env.add_fn("foreach", foreach, 2, ty_foreach);

		// Debugging functions.
		env.add_fn("dbg", dbg, 1, ty_inherit_first);

		env
	}

	/// Create a child environment.
	pub fn child(&self) -> Env<'_> {
		Env {
			bindings: HashMap::new(),
			parent: Some(self),
		}
	}

	/// Add a variable to the environment.
	pub fn add_var(&mut self, name: &str, value: Primitive) -> Option<Binding> {
		self.bindings.insert(name.to_owned(), Binding::Var(value))
	}

	/// Add a function to the environment.
	pub fn add_fn(
		&mut self,
		name: &str,
		op: Op,
		expected_args: usize,
		ty_checker: TypeChecker,
	) -> Option<Binding> {
		self.bindings.insert(
			name.to_owned(),
			Binding::Fn(FunctionDef {
				name: name.to_owned(),
				expected_args,
				ty_checker,
				op,
			}),
		)
	}

	/// Get a binding from the environment, walking up the scopes.
	pub fn get(&self, name: &str) -> Option<Binding> {
		self.bindings
			.get(name)
			.cloned()
			.or_else(|| self.parent.and_then(|parent| parent.get(name)))
	}
}

/// Check the number of args provided to the function.
fn check_num_args(name: &str, args: &[Expr], expected: usize) -> Result<()> {
	let given = args.len();

	match expected.cmp(&given) {
		Ordering::Equal => Ok(()),
		Ordering::Less => Err(Error::TooManyArgs {
			name: name.to_string(),
			expected,
			given,
		}),
		Ordering::Greater => Err(Error::NotEnoughArgs {
			name: name.to_string(),
			expected,
			given,
		}),
	}
}

/// Partially evaluate a binary operation on primitives.
pub fn partially_evaluate(env: &Env, fn_name: &str, arg: Expr) -> Result<Expr> {
	let var_name = "x";
	let var = Ident(String::from(var_name));
	let func = Ident(String::from(fn_name));
	// @Note - we put a placeholder var in the first operand of the binary
	// function lambda to make higher-order functions read better.
	// e.g. `(filter (lt 3) [])` would actually check if array elements are
	// greater than 3 if we put the placeholder var second
	let op =
		StructFunction::new(func, vec![Primitive(Identifier(var.clone())), arg]).resolve(env)?;
	let lambda: Expr = StructLambda::new(var, op).into();
	lambda.get_type()?;
	Ok(lambda)
}

pub fn upcast(i: i64) -> Primitive {
	Float(F64::new(i as f64).unwrap())
}

/// Define binary operations on primitives.
fn binary_primitive_op<F>(name: &'static str, env: &Env, args: &[Expr], op: F) -> Result<Expr>
where
	F: FnOnce(Primitive, Primitive) -> Result<Primitive>,
{
	if args.len() == 1 {
		return partially_evaluate(env, name, args[0].clone());
	}

	check_num_args(name, args, 2)?;

	let arg_1 = match env.visit_expr(args[0].clone())? {
		Primitive(p) => p,
		_ => return Err(Error::BadType(name)),
	};

	let arg_2 = match env.visit_expr(args[1].clone())? {
		Primitive(p) => p,
		_ => return Err(Error::BadType(name)),
	};

	let primitive = match (&arg_1, &arg_2) {
		(Int(i), Float(_)) => op(upcast(*i), arg_2)?,
		(Float(_), Int(i)) => op(arg_1, upcast(*i))?,
		_ => op(arg_1, arg_2)?,
	};

	Ok(Primitive(primitive))
}

/// Define unary operations on primitives.
fn unary_primitive_op<F>(name: &'static str, env: &Env, args: &[Expr], op: F) -> Result<Expr>
where
	F: FnOnce(Primitive) -> Result<Primitive>,
{
	check_num_args(name, args, 1)?;

	let primitive = match env.visit_expr(args[0].clone())? {
		Primitive(arg) => arg,
		_ => return Err(Error::BadType(name)),
	};

	Ok(Expr::Primitive(op(primitive)?))
}

/// Define unary operations on arrays.
fn unary_array_op<F>(name: &'static str, env: &Env, args: &[Expr], op: F) -> Result<Expr>
where
	F: FnOnce(ArrayType) -> Result<Expr>,
{
	check_num_args(name, args, 1)?;

	let arr = match env.visit_expr(args[0].clone())? {
		Array(a) => array_type(&a.elts[..])?,
		_ => return Err(Error::BadType(name)),
	};

	op(arr)
}

/// Define a higher-order operation over arrays.
fn higher_order_array_op<F>(name: &'static str, env: &Env, args: &[Expr], op: F) -> Result<Expr>
where
	F: FnOnce(ArrayType, Ident, Function) -> Result<Expr>,
{
	check_num_args(name, args, 2)?;

	let (ident, body) = match env.visit_expr(args[0].clone())? {
		Lambda(l) => (l.arg, l.body),
		_ => return Err(Error::BadType(name)),
	};

	let arr = match env.visit_expr(args[1].clone())? {
		Array(a) => array_type(&a.elts[..])?,
		_ => return Err(Error::BadType(name)),
	};

	op(arr, ident, body)
}

/// A fully-typed array.
enum ArrayType {
	/// An array of ints.
	Int(Vec<i64>),

	/// An array of floats.
	Float(Vec<F64>),

	/// An array of bools.
	Bool(Vec<bool>),

	/// An array of datetimes.
	DateTime(Vec<Zoned>),

	/// An array of time spans.
	Span(Vec<Span>),

	/// An empty array (no type hints).
	Empty,
}

/// Process an array into a singular type, or error out.
fn array_type(arr: &[Primitive]) -> Result<ArrayType> {
	if arr.is_empty() {
		return Ok(ArrayType::Empty);
	}

	match &arr[0] {
		Int(_) => {
			let mut result: Vec<i64> = Vec::with_capacity(arr.len());
			for elem in arr {
				if let Int(val) = elem {
					result.push(*val);
				} else {
					return Err(Error::InconsistentArrayTypes);
				}
			}
			Ok(ArrayType::Int(result))
		}
		Float(_) => {
			let mut result: Vec<F64> = Vec::with_capacity(arr.len());
			for elem in arr {
				if let Float(val) = elem {
					result.push(*val);
				} else {
					return Err(Error::InconsistentArrayTypes);
				}
			}
			Ok(ArrayType::Float(result))
		}
		Bool(_) => {
			let mut result: Vec<bool> = Vec::with_capacity(arr.len());
			for elem in arr {
				if let Bool(val) = elem {
					result.push(*val);
				} else {
					return Err(Error::InconsistentArrayTypes);
				}
			}
			Ok(ArrayType::Bool(result))
		}
		DateTime(_) => {
			let mut result: Vec<Zoned> = Vec::with_capacity(arr.len());
			for elem in arr {
				if let DateTime(val) = elem {
					result.push(val.clone());
				} else {
					return Err(Error::InconsistentArrayTypes);
				}
			}
			Ok(ArrayType::DateTime(result))
		}
		Span(_) => {
			let mut result: Vec<Span> = Vec::with_capacity(arr.len());
			for elem in arr {
				if let Span(val) = elem {
					result.push(*val);
				} else {
					return Err(Error::InconsistentArrayTypes);
				}
			}
			Ok(ArrayType::Span(result))
		}

		Identifier(_) => unimplemented!("we don't currently support idents in arrays"),
	}
}

/// Evaluate the lambda, injecting into the environment.
fn eval_lambda(env: &Env, ident: &Ident, val: Primitive, body: Function) -> Result<Expr> {
	let mut child = env.child();

	if child.add_var(&ident.0, val).is_some() {
		return Err(Error::AlreadyBound);
	}

	child.visit_function(body)
}

#[allow(clippy::bool_comparison)]
fn gt(env: &Env, args: &[Expr]) -> Result<Expr> {
	let name = "gt";

	let op = |arg_1, arg_2| match (arg_1, arg_2) {
		(Int(arg_1), Int(arg_2)) => Ok(Bool(arg_1 > arg_2)),
		(Float(arg_1), Float(arg_2)) => Ok(Bool(arg_1 > arg_2)),
		(Bool(arg_1), Bool(arg_2)) => Ok(Bool(arg_1 > arg_2)),
		(DateTime(arg_1), DateTime(arg_2)) => Ok(Bool(arg_1 > arg_2)),
		(Span(arg_1), Span(arg_2)) => Ok(Bool(
			arg_1
				.compare(arg_2)
				.map_err(|err| Error::Datetime(err.to_string()))?
				.is_gt(),
		)),
		_ => unreachable!(),
	};

	binary_primitive_op(name, env, args, op)
}

#[allow(clippy::bool_comparison)]
fn lt(env: &Env, args: &[Expr]) -> Result<Expr> {
	let name = "lt";

	let op = |arg_1, arg_2| match (arg_1, arg_2) {
		(Int(arg_1), Int(arg_2)) => Ok(Bool(arg_1 < arg_2)),
		(Float(arg_1), Float(arg_2)) => Ok(Bool(arg_1 < arg_2)),
		(Bool(arg_1), Bool(arg_2)) => Ok(Bool(arg_1 < arg_2)),
		(DateTime(arg_1), DateTime(arg_2)) => Ok(Bool(arg_1 < arg_2)),
		(Span(arg_1), Span(arg_2)) => Ok(Bool(
			arg_1
				.compare(arg_2)
				.map_err(|err| Error::Datetime(err.to_string()))?
				.is_lt(),
		)),
		_ => unreachable!(),
	};

	binary_primitive_op(name, env, args, op)
}

#[allow(clippy::bool_comparison)]
fn gte(env: &Env, args: &[Expr]) -> Result<Expr> {
	let name = "gte";

	let op = |arg_1, arg_2| match (arg_1, arg_2) {
		(Int(arg_1), Int(arg_2)) => Ok(Bool(arg_1 >= arg_2)),
		(Float(arg_1), Float(arg_2)) => Ok(Bool(arg_1 >= arg_2)),
		(Bool(arg_1), Bool(arg_2)) => Ok(Bool(arg_1 >= arg_2)),
		(DateTime(arg_1), DateTime(arg_2)) => Ok(Bool(arg_1 >= arg_2)),
		(Span(arg_1), Span(arg_2)) => Ok(Bool(
			arg_1
				.compare(arg_2)
				.map_err(|err| Error::Datetime(err.to_string()))?
				.is_ge(),
		)),
		_ => unreachable!(),
	};

	binary_primitive_op(name, env, args, op)
}

#[allow(clippy::bool_comparison)]
fn lte(env: &Env, args: &[Expr]) -> Result<Expr> {
	let name = "lte";

	let op = |arg_1, arg_2| match (arg_1, arg_2) {
		(Int(arg_1), Int(arg_2)) => Ok(Bool(arg_1 <= arg_2)),
		(Float(arg_1), Float(arg_2)) => Ok(Bool(arg_1 <= arg_2)),
		(Bool(arg_1), Bool(arg_2)) => Ok(Bool(arg_1 <= arg_2)),
		(DateTime(arg_1), DateTime(arg_2)) => Ok(Bool(arg_1 <= arg_2)),
		(Span(arg_1), Span(arg_2)) => Ok(Bool(
			arg_1
				.compare(arg_2)
				.map_err(|err| Error::Datetime(err.to_string()))?
				.is_le(),
		)),
		_ => unreachable!(),
	};

	binary_primitive_op(name, env, args, op)
}

#[allow(clippy::bool_comparison)]
fn eq(env: &Env, args: &[Expr]) -> Result<Expr> {
	let name = "eq";

	let op = |arg_1, arg_2| match (arg_1, arg_2) {
		(Int(arg_1), Int(arg_2)) => Ok(Bool(arg_1 == arg_2)),
		(Float(arg_1), Float(arg_2)) => Ok(Bool(arg_1 == arg_2)),
		(Bool(arg_1), Bool(arg_2)) => Ok(Bool(arg_1 == arg_2)),
		(DateTime(arg_1), DateTime(arg_2)) => Ok(Bool(arg_1 == arg_2)),
		(Span(arg_1), Span(arg_2)) => Ok(Bool(
			arg_1
				.compare(arg_2)
				.map_err(|err| Error::Datetime(err.to_string()))?
				.is_eq(),
		)),
		_ => unreachable!(),
	};

	binary_primitive_op(name, env, args, op)
}

#[allow(clippy::bool_comparison)]
fn neq(env: &Env, args: &[Expr]) -> Result<Expr> {
	let name = "neq";

	let op = |arg_1, arg_2| match (arg_1, arg_2) {
		(Int(arg_1), Int(arg_2)) => Ok(Bool(arg_1 != arg_2)),
		(Float(arg_1), Float(arg_2)) => Ok(Bool(arg_1 != arg_2)),
		(Bool(arg_1), Bool(arg_2)) => Ok(Bool(arg_1 != arg_2)),
		(DateTime(arg_1), DateTime(arg_2)) => Ok(Bool(arg_1 != arg_2)),
		(Span(arg_1), Span(arg_2)) => Ok(Bool(
			arg_1
				.compare(arg_2)
				.map_err(|err| Error::Datetime(err.to_string()))?
				.is_ne(),
		)),
		_ => unreachable!(),
	};

	binary_primitive_op(name, env, args, op)
}

// Adds numbers or adds a span of time to a datetime (the latter use case is *not* commutative)
// Datetime addition will error for spans with units greater than days (which the parser should prevent)
fn add(env: &Env, args: &[Expr]) -> Result<Expr> {
	let name = "add";

	let op = |arg_1, arg_2| match (arg_1, arg_2) {
		(Int(arg_1), Int(arg_2)) => Ok(Int(arg_1 + arg_2)),
		(Float(arg_1), Float(arg_2)) => Ok(Float(arg_1 + arg_2)),
		// Span or DateTime can come first
		(DateTime(dt), Span(s)) | (Span(s), DateTime(dt)) => Ok(DateTime(
			dt.checked_add(s)
				.map_err(|err| Error::Datetime(err.to_string()))?,
		)),
		(Span(arg_1), Span(arg_2)) => Ok(Span(
			arg_1
				.checked_add(arg_2)
				.map_err(|err| Error::Datetime(err.to_string()))?,
		)),
		(_, _) => Err(Error::BadType(name)),
	};

	binary_primitive_op(name, env, args, op)
}

// Subtracts numbers or subtracts a span of time from a datetime
// Datetime addition will error for spans with units greater than days (which the parser should prevent)
// Do not use for finding the difference between two dateimes. The correct operation for "subtracting" two datetimes is "duration."
fn sub(env: &Env, args: &[Expr]) -> Result<Expr> {
	let name = "sub";

	let op = |arg_1, arg_2| match (arg_1, arg_2) {
		(Int(arg_1), Int(arg_2)) => Ok(Int(arg_1 - arg_2)),
		(Float(arg_1), Float(arg_2)) => Ok(Float(arg_1 - arg_2)),
		(DateTime(arg_1), Span(arg_2)) => Ok(DateTime(
			arg_1
				.checked_sub(arg_2)
				.map_err(|err| Error::Datetime(err.to_string()))?,
		)),
		(Span(arg_1), Span(arg_2)) => Ok(Span(
			arg_1
				.checked_sub(arg_2)
				.map_err(|err| Error::Datetime(err.to_string()))?,
		)),
		(_, _) => Err(Error::BadType(name)),
	};

	binary_primitive_op(name, env, args, op)
}

// Attempts to divide the numbers, returning a float. If
// the divisor is zero, returns 0 instead
fn divz(env: &Env, args: &[Expr]) -> Result<Expr> {
	let name = "divz";

	let op = |arg_1, arg_2| match (arg_1, arg_2) {
		(Int(arg_1), Int(arg_2)) => Ok(if arg_2 == 0 {
			Float(F64::new(0.0)?)
		} else {
			let f_arg_1 = arg_1 as f64;
			let f_arg_2 = arg_2 as f64;
			Float(F64::new(f_arg_1 / f_arg_2)?)
		}),
		(Float(arg_1), Float(arg_2)) => Ok(if arg_2 == 0.0 {
			Float(arg_2)
		} else {
			Float(arg_1 / arg_2)
		}),
		(_, _) => Err(Error::BadType(name)),
	};

	binary_primitive_op(name, env, args, op)
}

// Finds the difference in time between two datetimes, in units no larger than hours (chosen for comparision safety)
fn duration(env: &Env, args: &[Expr]) -> Result<Expr> {
	let name = "duration";

	let op = |arg_1, arg_2| match (arg_1, arg_2) {
		(DateTime(arg_1), DateTime(arg_2)) => Ok(Span(
			arg_1
				.since(&arg_2)
				.map_err(|err| Error::Datetime(err.to_string()))?,
		)),
		(_, _) => Err(Error::BadType(name)),
	};

	binary_primitive_op(name, env, args, op)
}

fn and(env: &Env, args: &[Expr]) -> Result<Expr> {
	let name = "and";

	let op = |arg_1, arg_2| match (arg_1, arg_2) {
		(Bool(arg_1), Bool(arg_2)) => Ok(Bool(arg_1 && arg_2)),
		(_, _) => Err(Error::BadType(name)),
	};

	binary_primitive_op(name, env, args, op)
}

fn or(env: &Env, args: &[Expr]) -> Result<Expr> {
	let name = "or";

	let op = |arg_1, arg_2| match (arg_1, arg_2) {
		(Bool(arg_1), Bool(arg_2)) => Ok(Bool(arg_1 || arg_2)),
		(_, _) => Err(Error::BadType(name)),
	};

	binary_primitive_op(name, env, args, op)
}

fn not(env: &Env, args: &[Expr]) -> Result<Expr> {
	let name = "not";

	let op = |arg| match arg {
		Int(_) => Err(Error::BadType(name)),
		Float(_) => Err(Error::BadType(name)),
		Bool(arg) => Ok(Primitive::Bool(arg.not())),
		DateTime(_) => Err(Error::BadType(name)),
		Span(_) => Err(Error::BadType(name)),
		Identifier(_) => unreachable!("no idents should be here"),
	};

	unary_primitive_op(name, env, args, op)
}

fn max(env: &Env, args: &[Expr]) -> Result<Expr> {
	let name = "max";

	let op = |arg| match arg {
		ArrayType::Int(ints) => ints
			.iter()
			.copied()
			.max()
			.ok_or(Error::NoMax)
			.map(|m| Primitive(Int(m))),

		ArrayType::Float(floats) => floats
			.iter()
			.copied()
			.max()
			.ok_or(Error::NoMax)
			.map(|m| Primitive(Float(m))),

		ArrayType::Bool(_) => Err(Error::BadType(name)),
		ArrayType::DateTime(_) => Err(Error::BadType(name)),
		ArrayType::Span(_) => Err(Error::BadType(name)),
		ArrayType::Empty => Err(Error::NoMax),
	};

	unary_array_op(name, env, args, op)
}

fn min(env: &Env, args: &[Expr]) -> Result<Expr> {
	let name = "min";

	let op = |arg| match arg {
		ArrayType::Int(ints) => ints
			.iter()
			.copied()
			.min()
			.ok_or(Error::NoMin)
			.map(|m| Primitive(Int(m))),

		ArrayType::Float(floats) => floats
			.iter()
			.copied()
			.min()
			.ok_or(Error::NoMin)
			.map(|m| Primitive(Float(m))),

		ArrayType::Bool(_) => Err(Error::BadType(name)),
		ArrayType::DateTime(_) => Err(Error::BadType(name)),
		ArrayType::Span(_) => Err(Error::BadType(name)),
		ArrayType::Empty => Err(Error::NoMin),
	};

	unary_array_op(name, env, args, op)
}

fn avg(env: &Env, args: &[Expr]) -> Result<Expr> {
	let name = "avg";

	let op = |arg| match arg {
		ArrayType::Int(ints) => {
			let count = ints.len() as i64;
			let sum = ints.iter().copied().sum::<i64>();
			Ok(Primitive(Float(F64::new(sum as f64 / count as f64)?)))
		}

		ArrayType::Float(floats) => {
			let count = floats.len() as i64;
			let sum = floats.iter().copied().sum::<F64>();
			Ok(Primitive(Float(F64::new(sum.into_inner() / count as f64)?)))
		}

		ArrayType::Bool(_) => Err(Error::BadType(name)),
		ArrayType::DateTime(_) => Err(Error::BadType(name)),
		ArrayType::Span(_) => Err(Error::BadType(name)),
		ArrayType::Empty => Err(Error::NoAvg),
	};

	unary_array_op(name, env, args, op)
}

fn median(env: &Env, args: &[Expr]) -> Result<Expr> {
	let name = "median";

	let op = |arg| match arg {
		ArrayType::Int(mut ints) => {
			ints.sort();
			let mid = ints.len() / 2;
			Ok(Primitive(Int(ints[mid])))
		}
		ArrayType::Float(mut floats) => {
			floats.sort();
			let mid = floats.len() / 2;
			Ok(Primitive(Float(floats[mid])))
		}
		ArrayType::Bool(_) => Err(Error::BadType(name)),
		ArrayType::DateTime(_) => Err(Error::BadType(name)),
		ArrayType::Span(_) => Err(Error::BadType(name)),
		ArrayType::Empty => Err(Error::NoMedian),
	};

	unary_array_op(name, env, args, op)
}

fn count(env: &Env, args: &[Expr]) -> Result<Expr> {
	let name = "count";

	let op = |arg| match arg {
		ArrayType::Int(ints) => Ok(Primitive(Int(ints.len() as i64))),
		ArrayType::Float(floats) => Ok(Primitive(Int(floats.len() as i64))),
		ArrayType::Bool(bools) => Ok(Primitive(Int(bools.len() as i64))),
		ArrayType::DateTime(dts) => Ok(Primitive(Int(dts.len() as i64))),
		ArrayType::Span(spans) => Ok(Primitive(Int(spans.len() as i64))),
		ArrayType::Empty => Ok(Primitive(Int(0))),
	};

	unary_array_op(name, env, args, op)
}

fn all(env: &Env, args: &[Expr]) -> Result<Expr> {
	let name = "all";

	let op = |arr, ident: Ident, body: Function| {
		let result = match arr {
			ArrayType::Int(ints) => ints
				.iter()
				.map(|val| eval_lambda(env, &ident, Int(*val), body.clone()))
				.process_results(|mut iter| {
					iter.all(|expr| matches!(expr, Primitive(Bool(true))))
				})?,
			ArrayType::Float(floats) => floats
				.iter()
				.map(|val| eval_lambda(env, &ident, Float(*val), body.clone()))
				.process_results(|mut iter| {
					iter.all(|expr| matches!(expr, Primitive(Bool(true))))
				})?,
			ArrayType::Bool(bools) => bools
				.iter()
				.map(|val| eval_lambda(env, &ident, Bool(*val), body.clone()))
				.process_results(|mut iter| {
					iter.all(|expr| matches!(expr, Primitive(Bool(true))))
				})?,
			ArrayType::DateTime(dts) => dts
				.iter()
				.map(|val| eval_lambda(env, &ident, DateTime(val.clone()), body.clone()))
				.process_results(|mut iter| {
					iter.all(|expr| matches!(expr, Primitive(Bool(true))))
				})?,
			ArrayType::Span(spans) => spans
				.iter()
				.map(|val| eval_lambda(env, &ident, Span(*val), body.clone()))
				.process_results(|mut iter| {
					iter.all(|expr| matches!(expr, Primitive(Bool(true))))
				})?,
			ArrayType::Empty => true,
		};

		Ok(Primitive(Bool(result)))
	};

	higher_order_array_op(name, env, args, op)
}

fn nall(env: &Env, args: &[Expr]) -> Result<Expr> {
	let name = "nall";

	let op = |arr, ident: Ident, body: Function| {
		let result = match arr {
			ArrayType::Int(ints) => ints
				.iter()
				.map(|val| eval_lambda(env, &ident, Int(*val), body.clone()))
				.process_results(|mut iter| {
					iter.all(|expr| matches!(expr, Primitive(Bool(true)))).not()
				})?,
			ArrayType::Float(floats) => floats
				.iter()
				.map(|val| eval_lambda(env, &ident, Float(*val), body.clone()))
				.process_results(|mut iter| {
					iter.all(|expr| matches!(expr, Primitive(Bool(true)))).not()
				})?,
			ArrayType::Bool(bools) => bools
				.iter()
				.map(|val| eval_lambda(env, &ident, Bool(*val), body.clone()))
				.process_results(|mut iter| {
					iter.all(|expr| matches!(expr, Primitive(Bool(true)))).not()
				})?,
			ArrayType::DateTime(dts) => dts
				.iter()
				.map(|val| eval_lambda(env, &ident, DateTime(val.clone()), body.clone()))
				.process_results(|mut iter| {
					iter.all(|expr| matches!(expr, Primitive(Bool(true)))).not()
				})?,
			ArrayType::Span(spans) => spans
				.iter()
				.map(|val| eval_lambda(env, &ident, Span(*val), body.clone()))
				.process_results(|mut iter| {
					iter.all(|expr| matches!(expr, Primitive(Bool(true)))).not()
				})?,
			ArrayType::Empty => false,
		};

		Ok(Primitive(Bool(result)))
	};

	higher_order_array_op(name, env, args, op)
}

fn some(env: &Env, args: &[Expr]) -> Result<Expr> {
	let name = "some";

	let op = |arr, ident: Ident, body: Function| {
		let result = match arr {
			ArrayType::Int(ints) => ints
				.iter()
				.map(|val| eval_lambda(env, &ident, Int(*val), body.clone()))
				.process_results(|mut iter| {
					iter.any(|expr| matches!(expr, Primitive(Bool(true))))
				})?,
			ArrayType::Float(floats) => floats
				.iter()
				.map(|val| eval_lambda(env, &ident, Float(*val), body.clone()))
				.process_results(|mut iter| {
					iter.any(|expr| matches!(expr, Primitive(Bool(true))))
				})?,
			ArrayType::Bool(bools) => bools
				.iter()
				.map(|val| eval_lambda(env, &ident, Bool(*val), body.clone()))
				.process_results(|mut iter| {
					iter.any(|expr| matches!(expr, Primitive(Bool(true))))
				})?,
			ArrayType::DateTime(dts) => dts
				.iter()
				.map(|val| eval_lambda(env, &ident, DateTime(val.clone()), body.clone()))
				.process_results(|mut iter| {
					iter.any(|expr| matches!(expr, Primitive(Bool(true))))
				})?,
			ArrayType::Span(spans) => spans
				.iter()
				.map(|val| eval_lambda(env, &ident, Span(*val), body.clone()))
				.process_results(|mut iter| {
					iter.any(|expr| matches!(expr, Primitive(Bool(true))))
				})?,
			ArrayType::Empty => false,
		};

		Ok(Primitive(Bool(result)))
	};

	higher_order_array_op(name, env, args, op)
}

fn none(env: &Env, args: &[Expr]) -> Result<Expr> {
	let name = "none";

	let op = |arr, ident: Ident, body: Function| {
		let result = match arr {
			ArrayType::Int(ints) => ints
				.iter()
				.map(|val| eval_lambda(env, &ident, Int(*val), body.clone()))
				.process_results(|mut iter| {
					iter.any(|expr| matches!(expr, Primitive(Bool(true)))).not()
				})?,
			ArrayType::Float(floats) => floats
				.iter()
				.map(|val| eval_lambda(env, &ident, Float(*val), body.clone()))
				.process_results(|mut iter| {
					iter.any(|expr| matches!(expr, Primitive(Bool(true)))).not()
				})?,
			ArrayType::Bool(bools) => bools
				.iter()
				.map(|val| eval_lambda(env, &ident, Bool(*val), body.clone()))
				.process_results(|mut iter| {
					iter.any(|expr| matches!(expr, Primitive(Bool(true)))).not()
				})?,
			ArrayType::DateTime(dts) => dts
				.iter()
				.map(|val| eval_lambda(env, &ident, DateTime(val.clone()), body.clone()))
				.process_results(|mut iter| {
					iter.any(|expr| matches!(expr, Primitive(Bool(true)))).not()
				})?,
			ArrayType::Span(spans) => spans
				.iter()
				.map(|val| eval_lambda(env, &ident, Span(*val), body.clone()))
				.process_results(|mut iter| {
					iter.any(|expr| matches!(expr, Primitive(Bool(true)))).not()
				})?,
			ArrayType::Empty => true,
		};

		Ok(Primitive(Bool(result)))
	};

	higher_order_array_op(name, env, args, op)
}

fn filter(env: &Env, args: &[Expr]) -> Result<Expr> {
	let name = "filter";

	let op = |arr, ident: Ident, body: Function| {
		let arr = match arr {
			ArrayType::Int(ints) => ints
				.iter()
				.map(|val| Ok((val, eval_lambda(env, &ident, Int(*val), body.clone()))))
				.filter_map_ok(|(val, expr)| {
					if let Ok(Primitive(Bool(true))) = expr {
						Some(Primitive::Int(*val))
					} else {
						None
					}
				})
				.collect::<Result<Vec<_>>>()?,
			ArrayType::Float(floats) => floats
				.iter()
				.map(|val| Ok((val, eval_lambda(env, &ident, Float(*val), body.clone()))))
				.filter_map_ok(|(val, expr)| {
					if let Ok(Primitive(Bool(true))) = expr {
						Some(Primitive::Float(*val))
					} else {
						None
					}
				})
				.collect::<Result<Vec<_>>>()?,
			ArrayType::Bool(bools) => bools
				.iter()
				.map(|val| Ok((val, eval_lambda(env, &ident, Bool(*val), body.clone()))))
				.filter_map_ok(|(val, expr)| {
					if let Ok(Primitive(Bool(true))) = expr {
						Some(Primitive::Bool(*val))
					} else {
						None
					}
				})
				.collect::<Result<Vec<_>>>()?,
			ArrayType::DateTime(dts) => dts
				.iter()
				.map(|val| {
					Ok((
						val,
						eval_lambda(env, &ident, DateTime(val.clone()), body.clone()),
					))
				})
				.filter_map_ok(|(val, expr)| {
					if let Ok(Primitive(Bool(true))) = expr {
						Some(Primitive::DateTime(val.clone()))
					} else {
						None
					}
				})
				.collect::<Result<Vec<_>>>()?,
			ArrayType::Span(spans) => spans
				.iter()
				.map(|val| Ok((val, eval_lambda(env, &ident, Span(*val), body.clone()))))
				.filter_map_ok(|(val, expr)| {
					if let Ok(Primitive(Bool(true))) = expr {
						Some(Primitive::Span(*val))
					} else {
						None
					}
				})
				.collect::<Result<Vec<_>>>()?,
			ArrayType::Empty => Vec::new(),
		};

		Ok(StructArray::new(arr).into())
	};

	higher_order_array_op(name, env, args, op)
}

fn foreach(env: &Env, args: &[Expr]) -> Result<Expr> {
	let name = "foreach";

	let op = |arr, ident: Ident, body: Function| {
		let arr = match arr {
			ArrayType::Int(ints) => ints
				.iter()
				.map(|val| eval_lambda(env, &ident, Int(*val), body.clone()))
				.map(|expr| match expr {
					Ok(Primitive(inner)) => Ok(inner),
					Ok(_) => Err(Error::BadType(name)),
					Err(err) => Err(err),
				})
				.collect::<Result<Vec<_>>>()?,
			ArrayType::Float(floats) => floats
				.iter()
				.map(|val| eval_lambda(env, &ident, Float(*val), body.clone()))
				.map(|expr| match expr {
					Ok(Primitive(inner)) => Ok(inner),
					Ok(_) => Err(Error::BadType(name)),
					Err(err) => Err(err),
				})
				.collect::<Result<Vec<_>>>()?,
			ArrayType::Bool(bools) => bools
				.iter()
				.map(|val| eval_lambda(env, &ident, Bool(*val), body.clone()))
				.map(|expr| match expr {
					Ok(Primitive(inner)) => Ok(inner),
					Ok(_) => Err(Error::BadType(name)),
					Err(err) => Err(err),
				})
				.collect::<Result<Vec<_>>>()?,
			ArrayType::DateTime(dts) => dts
				.iter()
				.map(|val| eval_lambda(env, &ident, DateTime(val.clone()), body.clone()))
				.map(|expr| match expr {
					Ok(Primitive(inner)) => Ok(inner),
					Ok(_) => Err(Error::BadType(name)),
					Err(err) => Err(err),
				})
				.collect::<Result<Vec<_>>>()?,
			ArrayType::Span(spans) => spans
				.iter()
				.map(|val| eval_lambda(env, &ident, Span(*val), body.clone()))
				.map(|expr| match expr {
					Ok(Primitive(inner)) => Ok(inner),
					Ok(_) => Err(Error::BadType(name)),
					Err(err) => Err(err),
				})
				.collect::<Result<Vec<_>>>()?,
			ArrayType::Empty => Vec::new(),
		};

		Ok(StructArray::new(arr).into())
	};

	higher_order_array_op(name, env, args, op)
}

fn dbg(env: &Env, args: &[Expr]) -> Result<Expr> {
	let name = "dbg";
	check_num_args(name, args, 1)?;
	let arg = &args[0];
	let result = env.visit_expr(arg.clone())?;
	log::debug!("{arg} = {result}");
	Ok(result)
}
