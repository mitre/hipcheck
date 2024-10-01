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
	fn visit_lambda(&self, func: Lambda) -> Result<Expr> {
		println!("Visiting lambda: {func:?}");
		todo!()
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
		let fn_ty = func.get_type()?;
		let Type::Function(ft) = fn_ty else {
			return Err(Error::BadType("i don't know how we got here"));
		};
		if let FuncReturnType::Dynamic(fn_ty_fn) = ft.return_ty {
			if let Err(e) = (fn_ty_fn)(&ft.arg_tys) {
				println!("Invalid type, e: {e:?}");
				return Err(e);
			}
		}
		Ok(ft.into())
	}
	fn visit_lambda(&self, lamb: &Lambda) -> Result<Type> {
		todo!()
	}
	fn visit_json_pointer(&self, jp: &JsonPointer) -> Result<Type> {
		return Err(Error::BadType("can't type JSON pointers"));
	}
}

pub struct TypeFixer {}
impl ExprMutator for TypeFixer {
	fn visit_function(&self, mut func: Function) -> Result<Expr> {
		// @FollowUp - should the ExprMutator be combined into this?
		func.args = func
			.args
			.drain(..)
			.map(|a| self.visit_expr(a))
			.collect::<Result<Vec<Expr>>>()?;
		let fn_ty = func.get_type()?;
		match fn_ty {
			Type::Function(ft) => match ft.return_ty {
				FuncReturnType::Static(_) => (),
				FuncReturnType::Dynamic(fn_ty_fn) => {
					if let Err(e) = (fn_ty_fn)(&ft.arg_tys) {
						println!("Invalid type, e: {e:?}");
					}
				}
			},
			_ => todo!(),
		}
		Ok(func.into())
	}
}
