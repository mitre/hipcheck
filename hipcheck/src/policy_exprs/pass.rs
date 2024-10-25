// SPDX-License-Identifier: Apache-2.0

use crate::policy_exprs::{
	env::Env,
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
		lamb.body = Box::new(self.visit_expr(*lamb.body.clone())?);
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
