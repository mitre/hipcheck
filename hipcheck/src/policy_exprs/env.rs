// SPDX-License-Identifier: Apache-2.0

use crate::policy_exprs::{
	expr::{FuncReturnType, Op, OpInfo, PrimitiveType, ReturnableType, Type},
	Array as StructArray, Error, Expr, ExprVisitor, Function as StructFunction, Ident,
	Lambda as StructLambda, Primitive, Result, F64,
};
use itertools::Itertools as _;
use jiff::{Span, Zoned};
use std::{
	cmp::{Ordering, PartialEq},
	collections::HashMap,
	ops::Not as _,
};
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
	Fn(OpInfo),

	/// A primitive value.
	Var(Primitive),
}

fn ty_filter(args: &[Type]) -> Result<ReturnableType> {
	let Some(wrapped_arr_ty) = args.get(1) else {
		return Err(Error::MissingArgs);
	};
	let Type::Array(arr_ty) = wrapped_arr_ty else {
		return Err(Error::BadType("todo!"));
	};
	Ok(ReturnableType::Array(*arr_ty))
}

// Type of dynamic function is dependent on first arg
fn ty_inherit_first(args: &[Type]) -> Result<ReturnableType> {
	let Some(child_ty) = args.get(0) else {
		return Err(Error::MissingArgs);
	};
	child_ty.try_into()
}

fn ty_from_first_arr(args: &[Type]) -> Result<ReturnableType> {
	let Some(Type::Array(arr_ty)) = args.get(0) else {
		return Err(Error::MissingArgs);
	};
	Ok(match arr_ty {
		None => ReturnableType::Unknown,
		Some(p_ty) => ReturnableType::Primitive(*p_ty),
	})
}

// @Note - the logic would be a lot simpler if we received the
// expressions themselves instead of the types. This is because
// we can't `match` on our PTY constants
fn ty_arithmetic_binary_op(args: &[Type]) -> Result<ReturnableType> {
	// resolves the two operands
	let Some(ty_1) = args.get(0) else {
		return Err(Error::MissingArgs);
	};
	let Some(ty_2) = args.get(1) else {
		return Err(Error::MissingArgs);
	};
	// ensure they both result in primitive types or unknown
	let opt_ty_1: Option<PrimitiveType> = match ty_1.try_into()? {
		ReturnableType::Primitive(p) => Some(p),
		ReturnableType::Array(_) => {
			return Err(Error::BadType("todo!"));
		}
		ReturnableType::Unknown => None,
	};
	let opt_ty_2: Option<PrimitiveType> = match ty_2.try_into()? {
		ReturnableType::Primitive(p) => Some(p),
		ReturnableType::Array(_) => {
			return Err(Error::BadType("todo!"));
		}
		ReturnableType::Unknown => None,
	};
	use PrimitiveType::*;
	use ReturnableType::*;
	if opt_ty_1.is_none() || opt_ty_2.is_none() {
		let (single_ty, first_op) = match (opt_ty_1, opt_ty_2) {
			// if both are unknown, return unknown
			(None, None) => {
				return Ok(Unknown);
			}
			(Some(t), None) => (t, true),
			(None, Some(t)) => (t, false),
			_ => unreachable!(),
		};
		Ok(match (single_ty, first_op) {
			(Float, _) => Primitive(Float),
			(Int, _) => Primitive(Int),
			(Span, true) => Primitive(Span), // span in first position indicates span arithmetic
			(Span, false) => Unknown,        // span in second position could be datetime or span arithmetic
			(DateTime, true) => Primitive(DateTime), // expect a span in second position
			_ => {
				return Err(Error::BadType("todo!"));
			}
		})
	} else {
		let ty_1 = opt_ty_1.unwrap();
		let ty_2 = opt_ty_2.unwrap();
		Ok(match (ty_1, ty_2) {
			(Float, _) | (_, Float) => Primitive(Float),
			(Int, _) | (_, Int) => Primitive(Int),
			(DateTime, Span) => Primitive(DateTime),
			(Span, _) | (_, Span) => Primitive(Span),
			_ => {
				return Err(Error::BadType("todo!"));
			}
		})
	}
}

fn ty_foreach(args: &[Type]) -> Result<ReturnableType> {
	println!("args: {:?}", args);
	todo!()
}

impl<'parent> Env<'parent> {
	/// Create an empty environment.
	fn empty() -> Self {
		Env {
			bindings: HashMap::new(),
			parent: None,
		}
	}

	/// Create the standard environment.
	pub fn std() -> Self {
		use PrimitiveType::*;
		let mut env = Env::empty();

		let ret_bool = FuncReturnType::Static(Bool.into());
		let ret_int = FuncReturnType::Static(Int.into());
		let ret_span = FuncReturnType::Static(Span.into());
		let ret_float = FuncReturnType::Static(Float.into());

		// Comparison functions.
		env.add_fn("gt", gt, ret_bool);
		env.add_fn("lt", lt, ret_bool);
		env.add_fn("gte", gte, ret_bool);
		env.add_fn("lte", lte, ret_bool);
		env.add_fn("eq", eq, ret_bool);
		env.add_fn("neq", neq, ret_bool);

		// Math functions.
		env.add_fn("add", add, FuncReturnType::Dynamic(ty_arithmetic_binary_op));
		env.add_fn("sub", sub, FuncReturnType::Dynamic(ty_arithmetic_binary_op));
		env.add_fn("divz", divz, ret_float);

		// Additional datetime math functions
		env.add_fn("duration", duration, ret_span);

		// Logical functions.
		env.add_fn("and", and, ret_bool);
		env.add_fn("or", or, ret_bool);
		env.add_fn("not", not, ret_bool);

		// Array math functions.
		env.add_fn("max", max, FuncReturnType::Dynamic(ty_from_first_arr));
		env.add_fn("min", min, FuncReturnType::Dynamic(ty_from_first_arr));
		env.add_fn("avg", avg, ret_float);
		env.add_fn("median", median, FuncReturnType::Dynamic(ty_from_first_arr));
		env.add_fn("count", count, ret_int);

		// Array logic functions.
		env.add_fn("all", all, ret_bool);
		env.add_fn("nall", nall, ret_bool);
		env.add_fn("some", some, ret_bool);
		env.add_fn("none", none, ret_bool);

		// Array higher-order functions.
		env.add_fn("filter", filter, FuncReturnType::Dynamic(ty_filter));
		env.add_fn("foreach", foreach, FuncReturnType::Dynamic(ty_foreach));

		// Debugging functions.
		env.add_fn("dbg", dbg, FuncReturnType::Dynamic(ty_inherit_first));

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
	pub fn add_fn(&mut self, name: &str, op: Op, fn_ty: FuncReturnType) -> Option<Binding> {
		self.bindings
			.insert(name.to_owned(), Binding::Fn(OpInfo { fn_ty, op }))
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
fn partially_evaluate(fn_name: &'static str, arg: Expr) -> Result<Expr> {
	let var_name = "x";
	let var = Ident(String::from(var_name));
	let func = Ident(String::from(fn_name));
	let op = StructFunction::new(func, vec![Primitive(Identifier(var.clone())), arg]).into();
	let lambda = StructLambda::new(var, Box::new(op)).into();
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
		return partially_evaluate(name, args[0].clone());
	}

	check_num_args(name, args, 2)?;

	let arg_1 = match env.visit_expr(&args[0])? {
		Primitive(p) => p,
		_ => return Err(Error::BadType(name)),
	};

	let arg_2 = match env.visit_expr(&args[1])? {
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

	let primitive = match env.visit_expr(&args[0])? {
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

	let arr = match env.visit_expr(&args[0])? {
		Array(a) => array_type(&a.elts[..])?,
		_ => return Err(Error::BadType(name)),
	};

	op(arr)
}

/// Define a higher-order operation over arrays.
fn higher_order_array_op<F>(name: &'static str, env: &Env, args: &[Expr], op: F) -> Result<Expr>
where
	F: FnOnce(ArrayType, Ident, Box<Expr>) -> Result<Expr>,
{
	check_num_args(name, args, 2)?;

	let (ident, body) = match env.visit_expr(&args[0])? {
		Lambda(l) => (l.arg, l.body),
		_ => return Err(Error::BadType(name)),
	};

	let arr = match env.visit_expr(&args[1])? {
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
fn eval_lambda(env: &Env, ident: &Ident, val: Primitive, body: Expr) -> Result<Expr> {
	let mut child = env.child();

	if child.add_var(&ident.0, val).is_some() {
		return Err(Error::AlreadyBound);
	}

	child.visit_expr(&body)
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
		_ => unreachable!(),
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
		_ => unreachable!(),
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
		_ => unreachable!(),
	};

	binary_primitive_op(name, env, args, op)
}

// Finds the difference in time between two datetimes, in units of hours (chosen for comparision safety)
fn duration(env: &Env, args: &[Expr]) -> Result<Expr> {
	let name = "duration";

	let op = |arg_1, arg_2| match (arg_1, arg_2) {
		(DateTime(arg_1), DateTime(arg_2)) => Ok(Span(
			arg_1
				.since(&arg_2)
				.map_err(|err| Error::Datetime(err.to_string()))?,
		)),
		(_, _) => Err(Error::BadType(name)),
		_ => unreachable!(),
	};

	binary_primitive_op(name, env, args, op)
}

fn and(env: &Env, args: &[Expr]) -> Result<Expr> {
	let name = "and";

	let op = |arg_1, arg_2| match (arg_1, arg_2) {
		(Bool(arg_1), Bool(arg_2)) => Ok(Bool(arg_1 && arg_2)),
		(_, _) => Err(Error::BadType(name)),
		_ => unreachable!(),
	};

	binary_primitive_op(name, env, args, op)
}

fn or(env: &Env, args: &[Expr]) -> Result<Expr> {
	let name = "or";

	let op = |arg_1, arg_2| match (arg_1, arg_2) {
		(Bool(arg_1), Bool(arg_2)) => Ok(Bool(arg_1 || arg_2)),
		(_, _) => Err(Error::BadType(name)),
		_ => unreachable!(),
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

	let op = |arr, ident: Ident, body: Box<Expr>| {
		let result = match arr {
			ArrayType::Int(ints) => ints
				.iter()
				.map(|val| eval_lambda(env, &ident, Int(*val), (*body).clone()))
				.process_results(|mut iter| {
					iter.all(|expr| matches!(expr, Primitive(Bool(true))))
				})?,
			ArrayType::Float(floats) => floats
				.iter()
				.map(|val| eval_lambda(env, &ident, Float(*val), (*body).clone()))
				.process_results(|mut iter| {
					iter.all(|expr| matches!(expr, Primitive(Bool(true))))
				})?,
			ArrayType::Bool(bools) => bools
				.iter()
				.map(|val| eval_lambda(env, &ident, Bool(*val), (*body).clone()))
				.process_results(|mut iter| {
					iter.all(|expr| matches!(expr, Primitive(Bool(true))))
				})?,
			ArrayType::DateTime(dts) => dts
				.iter()
				.map(|val| eval_lambda(env, &ident, DateTime(val.clone()), (*body).clone()))
				.process_results(|mut iter| {
					iter.all(|expr| matches!(expr, Primitive(Bool(true))))
				})?,
			ArrayType::Span(spans) => spans
				.iter()
				.map(|val| eval_lambda(env, &ident, Span(*val), (*body).clone()))
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

	let op = |arr, ident: Ident, body: Box<Expr>| {
		let result = match arr {
			ArrayType::Int(ints) => ints
				.iter()
				.map(|val| eval_lambda(env, &ident, Int(*val), (*body).clone()))
				.process_results(|mut iter| {
					iter.all(|expr| matches!(expr, Primitive(Bool(true)))).not()
				})?,
			ArrayType::Float(floats) => floats
				.iter()
				.map(|val| eval_lambda(env, &ident, Float(*val), (*body).clone()))
				.process_results(|mut iter| {
					iter.all(|expr| matches!(expr, Primitive(Bool(true)))).not()
				})?,
			ArrayType::Bool(bools) => bools
				.iter()
				.map(|val| eval_lambda(env, &ident, Bool(*val), (*body).clone()))
				.process_results(|mut iter| {
					iter.all(|expr| matches!(expr, Primitive(Bool(true)))).not()
				})?,
			ArrayType::DateTime(dts) => dts
				.iter()
				.map(|val| eval_lambda(env, &ident, DateTime(val.clone()), (*body).clone()))
				.process_results(|mut iter| {
					iter.all(|expr| matches!(expr, Primitive(Bool(true)))).not()
				})?,
			ArrayType::Span(spans) => spans
				.iter()
				.map(|val| eval_lambda(env, &ident, Span(*val), (*body).clone()))
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

	let op = |arr, ident: Ident, body: Box<Expr>| {
		let result = match arr {
			ArrayType::Int(ints) => ints
				.iter()
				.map(|val| eval_lambda(env, &ident, Int(*val), (*body).clone()))
				.process_results(|mut iter| {
					iter.any(|expr| matches!(expr, Primitive(Bool(true))))
				})?,
			ArrayType::Float(floats) => floats
				.iter()
				.map(|val| eval_lambda(env, &ident, Float(*val), (*body).clone()))
				.process_results(|mut iter| {
					iter.any(|expr| matches!(expr, Primitive(Bool(true))))
				})?,
			ArrayType::Bool(bools) => bools
				.iter()
				.map(|val| eval_lambda(env, &ident, Bool(*val), (*body).clone()))
				.process_results(|mut iter| {
					iter.any(|expr| matches!(expr, Primitive(Bool(true))))
				})?,
			ArrayType::DateTime(dts) => dts
				.iter()
				.map(|val| eval_lambda(env, &ident, DateTime(val.clone()), (*body).clone()))
				.process_results(|mut iter| {
					iter.any(|expr| matches!(expr, Primitive(Bool(true))))
				})?,
			ArrayType::Span(spans) => spans
				.iter()
				.map(|val| eval_lambda(env, &ident, Span(*val), (*body).clone()))
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

	let op = |arr, ident: Ident, body: Box<Expr>| {
		let result = match arr {
			ArrayType::Int(ints) => ints
				.iter()
				.map(|val| eval_lambda(env, &ident, Int(*val), (*body).clone()))
				.process_results(|mut iter| {
					iter.any(|expr| matches!(expr, Primitive(Bool(true)))).not()
				})?,
			ArrayType::Float(floats) => floats
				.iter()
				.map(|val| eval_lambda(env, &ident, Float(*val), (*body).clone()))
				.process_results(|mut iter| {
					iter.any(|expr| matches!(expr, Primitive(Bool(true)))).not()
				})?,
			ArrayType::Bool(bools) => bools
				.iter()
				.map(|val| eval_lambda(env, &ident, Bool(*val), (*body).clone()))
				.process_results(|mut iter| {
					iter.any(|expr| matches!(expr, Primitive(Bool(true)))).not()
				})?,
			ArrayType::DateTime(dts) => dts
				.iter()
				.map(|val| eval_lambda(env, &ident, DateTime(val.clone()), (*body).clone()))
				.process_results(|mut iter| {
					iter.any(|expr| matches!(expr, Primitive(Bool(true)))).not()
				})?,
			ArrayType::Span(spans) => spans
				.iter()
				.map(|val| eval_lambda(env, &ident, Span(*val), (*body).clone()))
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

	let op = |arr, ident: Ident, body: Box<Expr>| {
		let arr = match arr {
			ArrayType::Int(ints) => ints
				.iter()
				.map(|val| Ok((val, eval_lambda(env, &ident, Int(*val), (*body).clone()))))
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
				.map(|val| Ok((val, eval_lambda(env, &ident, Float(*val), (*body).clone()))))
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
				.map(|val| Ok((val, eval_lambda(env, &ident, Bool(*val), (*body).clone()))))
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
						eval_lambda(env, &ident, DateTime(val.clone()), (*body).clone()),
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
				.map(|val| Ok((val, eval_lambda(env, &ident, Span(*val), (*body).clone()))))
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

	let op = |arr, ident: Ident, body: Box<Expr>| {
		let arr = match arr {
			ArrayType::Int(ints) => ints
				.iter()
				.map(|val| eval_lambda(env, &ident, Int(*val), (*body).clone()))
				.map(|expr| match expr {
					Ok(Primitive(inner)) => Ok(inner),
					Ok(_) => Err(Error::BadType(name)),
					Err(err) => Err(err),
				})
				.collect::<Result<Vec<_>>>()?,
			ArrayType::Float(floats) => floats
				.iter()
				.map(|val| eval_lambda(env, &ident, Float(*val), (*body).clone()))
				.map(|expr| match expr {
					Ok(Primitive(inner)) => Ok(inner),
					Ok(_) => Err(Error::BadType(name)),
					Err(err) => Err(err),
				})
				.collect::<Result<Vec<_>>>()?,
			ArrayType::Bool(bools) => bools
				.iter()
				.map(|val| eval_lambda(env, &ident, Bool(*val), (*body).clone()))
				.map(|expr| match expr {
					Ok(Primitive(inner)) => Ok(inner),
					Ok(_) => Err(Error::BadType(name)),
					Err(err) => Err(err),
				})
				.collect::<Result<Vec<_>>>()?,
			ArrayType::DateTime(dts) => dts
				.iter()
				.map(|val| eval_lambda(env, &ident, DateTime(val.clone()), (*body).clone()))
				.map(|expr| match expr {
					Ok(Primitive(inner)) => Ok(inner),
					Ok(_) => Err(Error::BadType(name)),
					Err(err) => Err(err),
				})
				.collect::<Result<Vec<_>>>()?,
			ArrayType::Span(spans) => spans
				.iter()
				.map(|val| eval_lambda(env, &ident, Span(*val), (*body).clone()))
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
	let result = env.visit_expr(arg)?;
	log::debug!("{arg} = {result}");
	Ok(result)
}
