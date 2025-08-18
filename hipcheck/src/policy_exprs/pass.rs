// SPDX-License-Identifier: Apache-2.0

use crate::policy_exprs::{
	env::{partially_evaluate, Env},
	error::{Error, Result},
	expr::*,
};

pub trait ExprVisitor<T> {
	fn visit_primitive(&self, prim: &Primitive) -> T;
	fn visit_array(&self, arr: &Array) -> T;
	fn visit_function(&self, func: &Function) -> T;
	fn visit_lambda(&self, func: &Lambda) -> T;
	fn visit_json_pointer(&self, func: &JsonPointer) -> T;
	fn visit_expr(&self, expr: &Expr) -> T {
		match expr {
			Expr::Primitive(a) => self.visit_primitive(a),
			Expr::Array(a) => self.visit_array(a),
			Expr::Function(a) => self.visit_function(a),
			Expr::Lambda(a) => self.visit_lambda(a),
			Expr::JsonPointer(a) => self.visit_json_pointer(a),
		}
	}
	fn run(&self, expr: &Expr) -> T {
		self.visit_expr(expr)
	}
}

pub trait ExprMutator {
	fn visit_primitive(&self, prim: Primitive) -> Result<Expr> {
		Ok(prim.into())
	}

	fn visit_array(&self, arr: Array) -> Result<Expr> {
		Ok(arr.into())
	}

	fn visit_function(&self, func: Function) -> Result<Expr> {
		let mut func = func;
		func.args = func
			.args
			.into_iter()
			.map(|a| self.visit_expr(a))
			.collect::<Result<Vec<Expr>>>()?;
		Ok(func.into())
	}

	fn visit_lambda(&self, lamb: Lambda) -> Result<Expr> {
		let mut lamb = lamb;
		lamb.body = match self.visit_function(lamb.body)? {
			Expr::Function(f) => f,
			// if the impl of `visit_function` returned a non-function, just return that
			other => return Ok(other),
		};
		Ok(lamb.into())
	}

	fn visit_json_pointer(&self, jp: JsonPointer) -> Result<Expr> {
		Ok(jp.into())
	}

	fn visit_expr(&self, expr: Expr) -> Result<Expr> {
		match expr {
			Expr::Primitive(a) => self.visit_primitive(a),
			Expr::Array(a) => self.visit_array(a),
			Expr::Function(a) => self.visit_function(a),
			Expr::Lambda(a) => self.visit_lambda(a),
			Expr::JsonPointer(a) => self.visit_json_pointer(a),
		}
	}

	fn run(&self, expr: Expr) -> Result<Expr> {
		self.visit_expr(expr)
	}
}

pub struct FunctionResolver {
	env: Env<'static>,
}

impl FunctionResolver {
	pub fn std() -> Self {
		FunctionResolver { env: Env::std() }
	}
}

impl ExprMutator for FunctionResolver {
	fn visit_function(&self, func: Function) -> Result<Expr> {
		let mut func = func.resolve(&self.env)?;
		func.args = func
			.args
			.drain(..)
			.map(|a| self.visit_expr(a))
			.collect::<Result<Vec<Expr>>>()?;
		Ok(Expr::Function(func))
	}

	fn visit_lambda(&self, mut func: Lambda) -> Result<Expr> {
		let new_body = self.visit_function(func.body)?;
		func.body = match new_body {
			Expr::Function(f) => f,
			_ => {
				return Err(Error::InternalError(
					"FunctionResolver's `visit_function` impl should always return a function"
						.to_owned()
						.into_boxed_str(),
				));
			}
		};
		Ok(Expr::Lambda(func))
	}
}

#[derive(Default)]
pub struct TypeChecker {}

impl ExprVisitor<Result<Type>> for TypeChecker {
	fn visit_primitive(&self, prim: &Primitive) -> Result<Type> {
		prim.get_type()
	}

	fn visit_array(&self, arr: &Array) -> Result<Type> {
		arr.get_type()
	}

	fn visit_function(&self, func: &Function) -> Result<Type> {
		func.args
			.iter()
			.map(|a| self.visit_expr(a))
			.collect::<Result<Vec<Type>>>()?;

		let Type::Function(ft) = func.get_type()? else {
			return Err(Error::InternalError(
				"expression must have been run through TypeFixer pass first"
					.to_owned()
					.into_boxed_str(),
			));
		};
		// Check that the arguments to the function are correct
		ft.get_return_type()?;
		Ok(ft.into())
	}

	fn visit_lambda(&self, lamb: &Lambda) -> Result<Type> {
		self.visit_function(&lamb.body)?;
		lamb.get_type()
	}

	fn visit_json_pointer(&self, jp: &JsonPointer) -> Result<Type> {
		jp.get_type()
	}
}

pub struct TypeFixer {
	env: Env<'static>,
}

impl TypeFixer {
	pub fn std() -> Self {
		TypeFixer { env: Env::std() }
	}
}

impl ExprMutator for TypeFixer {
	fn visit_function(&self, mut func: Function) -> Result<Expr> {
		// @FollowUp - should the FunctionResolver be combined into this?
		func.args = func
			.args
			.drain(..)
			.map(|a| self.visit_expr(a))
			.collect::<Result<Vec<Expr>>>()?;
		let fn_ty = func.get_type()?;
		// At this point we know it has info
		match fn_ty {
			Type::Function(_) => Ok(func.into()),
			Type::Lambda(_) => {
				// Have to feed the new expr through the current pass again
				// for any additional transformations
				let res = partially_evaluate(&self.env, &func.ident.0, func.args.remove(0))?;
				self.visit_expr(res)
			}
			_ => unreachable!(),
		}
	}
}
