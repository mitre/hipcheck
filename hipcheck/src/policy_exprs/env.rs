// SPDX-License-Identifier: Apache-2.0

use crate::policy_exprs::eval;
use crate::policy_exprs::Error;
use crate::policy_exprs::Expr;
use crate::policy_exprs::Ident;
use crate::policy_exprs::Primitive;
use crate::policy_exprs::Result;
use crate::policy_exprs::F64;
use itertools::Itertools as _;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::ops::Not as _;
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
	Fn(Op),

	/// A primitive value.
	Var(Primitive),
}

/// Helper type for operation function pointer.
type Op = fn(&Env, &[Expr]) -> Result<Expr>;

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
		let mut env = Env::empty();

		// Comparison functions.
		env.add_fn("gt", gt);
		env.add_fn("lt", lt);
		env.add_fn("gte", gte);
		env.add_fn("lte", lte);
		env.add_fn("eq", eq);
		env.add_fn("neq", neq);

		// Math functions.
		env.add_fn("add", add);
		env.add_fn("sub", sub);

		// Logical functions.
		env.add_fn("and", and);
		env.add_fn("or", or);
		env.add_fn("not", not);

		// Array math functions.
		env.add_fn("max", max);
		env.add_fn("min", min);
		env.add_fn("avg", avg);
		env.add_fn("median", median);
		env.add_fn("count", count);

		// Array logic functions.
		env.add_fn("all", all);
		env.add_fn("nall", nall);
		env.add_fn("some", some);
		env.add_fn("none", none);

		// Array higher-order functions.
		env.add_fn("filter", filter);
		env.add_fn("foreach", foreach);

		// Debugging functions.
		env.add_fn("dbg", dbg);

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
	pub fn add_fn(&mut self, name: &str, op: Op) -> Option<Binding> {
		self.bindings.insert(name.to_owned(), Binding::Fn(op))
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
	let op = Function(func, vec![Primitive(Identifier(var.clone())), arg]);
	let lambda = Lambda(var, Box::new(op));
	Ok(lambda)
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

	let arg_1 = match eval(env, &args[0])? {
		Primitive(p) => p,
		_ => return Err(Error::BadType(name)),
	};

	let arg_2 = match eval(env, &args[1])? {
		Primitive(p) => p,
		_ => return Err(Error::BadType(name)),
	};

	let primitive = match (&arg_1, &arg_2) {
		(Int(_), Int(_)) | (Float(_), Float(_)) | (Bool(_), Bool(_)) => op(arg_1, arg_2)?,
		_ => return Err(Error::BadType(name)),
	};

	Ok(Primitive(primitive))
}

/// Define unary operations on primitives.
fn unary_primitive_op<F>(name: &'static str, env: &Env, args: &[Expr], op: F) -> Result<Expr>
where
	F: FnOnce(Primitive) -> Result<Primitive>,
{
	check_num_args(name, args, 1)?;

	let primitive = match eval(env, &args[0])? {
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

	let arr = match eval(env, &args[0])? {
		Array(arg) => array_type(&arg[..])?,
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

	let (ident, body) = match eval(env, &args[0])? {
		Lambda(ident, body) => (ident, body),
		_ => return Err(Error::BadType(name)),
	};

	let arr = match eval(env, &args[1])? {
		Array(arr) => array_type(&arr[..])?,
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
		Identifier(_) => unimplemented!("we don't currently support idents in arrays"),
	}
}

/// Evaluate the lambda, injecting into the environment.
fn eval_lambda(env: &Env, ident: &Ident, val: Primitive, body: Expr) -> Result<Expr> {
	let mut child = env.child();

	if child.add_var(&ident.0, val).is_some() {
		return Err(Error::AlreadyBound);
	}

	eval(&child, &body)
}

#[allow(clippy::bool_comparison)]
fn gt(env: &Env, args: &[Expr]) -> Result<Expr> {
	let name = "gt";

	let op = |arg_1, arg_2| match (arg_1, arg_2) {
		(Int(arg_1), Int(arg_2)) => Ok(Bool(arg_1 > arg_2)),
		(Float(arg_1), Float(arg_2)) => Ok(Bool(arg_1 > arg_2)),
		(Bool(arg_1), Bool(arg_2)) => Ok(Bool(arg_1 > arg_2)),
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
		_ => unreachable!(),
	};

	binary_primitive_op(name, env, args, op)
}

fn add(env: &Env, args: &[Expr]) -> Result<Expr> {
	let name = "add";

	let op = |arg_1, arg_2| match (arg_1, arg_2) {
		(Int(arg_1), Int(arg_2)) => Ok(Int(arg_1 + arg_2)),
		(Float(arg_1), Float(arg_2)) => Ok(Float(arg_1 + arg_2)),
		(Bool(_), Bool(_)) => Err(Error::BadType(name)),
		_ => unreachable!(),
	};

	binary_primitive_op(name, env, args, op)
}

fn sub(env: &Env, args: &[Expr]) -> Result<Expr> {
	let name = "sub";

	let op = |arg_1, arg_2| match (arg_1, arg_2) {
		(Int(arg_1), Int(arg_2)) => Ok(Int(arg_1 - arg_2)),
		(Float(arg_1), Float(arg_2)) => Ok(Float(arg_1 - arg_2)),
		(Bool(_), Bool(_)) => Err(Error::BadType(name)),
		_ => unreachable!(),
	};

	binary_primitive_op(name, env, args, op)
}

fn and(env: &Env, args: &[Expr]) -> Result<Expr> {
	let name = "and";

	let op = |arg_1, arg_2| match (arg_1, arg_2) {
		(Int(_), Int(_)) => Err(Error::BadType(name)),
		(Float(_), Float(_)) => Err(Error::BadType(name)),
		(Bool(arg_1), Bool(arg_2)) => Ok(Bool(arg_1 && arg_2)),
		_ => unreachable!(),
	};

	binary_primitive_op(name, env, args, op)
}

fn or(env: &Env, args: &[Expr]) -> Result<Expr> {
	let name = "or";

	let op = |arg_1, arg_2| match (arg_1, arg_2) {
		(Int(_), Int(_)) => Err(Error::BadType(name)),
		(Float(_), Float(_)) => Err(Error::BadType(name)),
		(Bool(arg_1), Bool(arg_2)) => Ok(Bool(arg_1 || arg_2)),
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
			ArrayType::Empty => Vec::new(),
		};

		Ok(Array(arr))
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
			ArrayType::Empty => Vec::new(),
		};

		Ok(Array(arr))
	};

	higher_order_array_op(name, env, args, op)
}

fn dbg(env: &Env, args: &[Expr]) -> Result<Expr> {
	let name = "dbg";
	check_num_args(name, args, 1)?;
	let arg = &args[0];
	let result = eval(env, arg)?;
	log::debug!("{arg} = {result}");
	Ok(result)
}
