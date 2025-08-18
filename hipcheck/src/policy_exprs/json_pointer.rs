// SPDX-License-Identifier: Apache-2.0

use crate::policy_exprs::{
	error::{self, Error, JiffError, Result},
	expr::{Array, Expr, Primitive},
	ExprMutator, JsonPointer, LexingError,
};
use jiff::{tz::TimeZone, Timestamp, Zoned};
use ordered_float::NotNan;
use serde_json::Value;

/// Policy Expression stage that looks up JSON Pointers from the JSON `context`
/// value.
pub struct LookupJsonPointers<'ctx> {
	context: &'ctx Value,
}

impl<'ctx> LookupJsonPointers<'ctx> {
	pub fn with_context(context: &'ctx Value) -> Self {
		LookupJsonPointers { context }
	}
}

impl ExprMutator for LookupJsonPointers<'_> {
	fn visit_json_pointer(&self, mut jp: JsonPointer) -> Result<Expr> {
		let pointer = &jp.pointer;
		let context = self.context;
		let val = lookup_json_pointer(pointer, context)?;
		let expr = json_to_policy_expr(val, pointer, context)?;
		jp.value = Some(Box::new(expr));
		Ok(jp.into())
	}
}

/// Wrap serde_json's `Value::pointer` method to provide better error handling.
fn lookup_json_pointer<'val>(pointer: &str, context: &'val Value) -> Result<&'val Value> {
	// serde_json's JSON Pointer implementation does not distinguish between
	// syntax errors and lookup errors, so we check the syntax ourselves.
	// The only syntax error that serde_json currently recognizes is that a
	// non-empty pointer must start with the '/' character.
	if let Some(chr) = pointer.chars().next() {
		if chr != '/' {
			return Err(Error::JSONPointerInvalidSyntax {
				pointer: pointer.to_owned().into_boxed_str(),
			});
		}
	}
	match context.pointer(pointer) {
		Some(val) => Ok(val),
		None => Err(Error::JSONPointerLookupFailed {
			pointer: pointer.to_owned().into_boxed_str(),
			context: context.clone(),
		}),
	}
}

/// Attempt to interpret a JSON Value as a Policy Expression.
/// `pointer` and `context` are only passed in to provide more context in the
/// case of errors.
fn json_to_policy_expr(val: &Value, pointer: &str, context: &Value) -> Result<Expr> {
	match val {
		Value::Number(n) => {
			let not_nan = NotNan::new(n.as_f64().unwrap()).unwrap();
			Ok(Expr::Primitive(Primitive::Float(not_nan)))
		}
		Value::Bool(b) => Ok(Expr::Primitive(Primitive::Bool(*b))),
		Value::Array(a) => {
			// NOTE that this .collect will short circuit upon encountering the first error.
			let primitives = a
				.iter()
				.map(|v| json_array_item_to_policy_expr_primitive(v, pointer, context))
				.collect::<Result<Vec<Primitive>>>()?;
			// NOTE that no checking is done to confirm that all Primitives are the same type.
			// That would be a type error in the Policy Expr language.
			Ok(Array::new(primitives).into())
		}
		Value::Object(_) => Err(Error::JSONPointerUnrepresentableType {
			json_type: error::UnrepresentableJSONType::JSONObject,
			pointer: pointer.to_owned().into_boxed_str(),
			value: val.clone(),
			context: context.clone(),
		}),
		// Attempt to parse a datetime or span, and only error out if both fail.
		// TODO: Use JSON schema data to guide which of datetime or span to parse into.
		Value::String(s) => {
			if let Ok(date_time) = parse_datetime(s) {
				return Ok(Expr::Primitive(Primitive::DateTime(date_time)));
			}

			if let Ok(span) = parse_span(s) {
				return Ok(Expr::Primitive(Primitive::Span(span)));
			}

			Err(Error::JSONPointerUnrepresentableType {
				json_type: error::UnrepresentableJSONType::JSONString,
				pointer: pointer.to_owned().into_boxed_str(),
				value: val.clone(),
				context: context.clone(),
			})
		}
		Value::Null => Err(Error::JSONPointerUnrepresentableType {
			json_type: error::UnrepresentableJSONType::JSONNull,
			pointer: pointer.to_owned().into_boxed_str(),
			value: val.clone(),
			context: context.clone(),
		}),
	}
}

fn parse_datetime(s: &str) -> Result<jiff::Zoned> {
	let ts: Timestamp = s.parse().map_err(|jiff_error| {
		Error::Lex(LexingError::InvalidDatetime(
			s.to_owned(),
			JiffError::new(jiff_error),
		))
	})?;
	Ok(Zoned::new(ts, TimeZone::UTC))
}

fn parse_span(s: &str) -> Result<jiff::Span> {
	s.parse().map_err(|jiff_error| {
		Error::Lex(LexingError::InvalidSpan(
			s.to_owned(),
			JiffError::new(jiff_error),
		))
	})
}

fn json_array_item_to_policy_expr_primitive(
	v: &Value,
	pointer: &str,
	context: &Value,
) -> Result<Primitive> {
	let expr = json_to_policy_expr(v, pointer, context)?;
	match expr {
		Expr::Primitive(p) => Ok(p),
		_ => Err(Error::JSONPointerUnrepresentableType {
			json_type: error::UnrepresentableJSONType::NonPrimitiveInArray,
			pointer: pointer.to_owned().into_boxed_str(),
			value: v.clone(),
			context: context.clone(),
		}),
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::policy_exprs::expr::json_ptr;
	use crate::policy_exprs::F64;
	use test_log::test;

	#[test]
	fn json_timestamp() {
		let pointer = "/event_timestamp";
		let context = serde_json::json!({
			"event_timestamp": "2001-02-03T04:05:06+00:00"
		});
		let ts = "2001-02-03T04:05:06+00:00".parse().unwrap();
		let zo = Zoned::new(ts, TimeZone::UTC);
		let expected = Primitive::DateTime(zo).into();

		let val = lookup_json_pointer(pointer, &context).unwrap();
		let result = json_to_policy_expr(val, pointer, &context);
		assert_eq!(result, Ok(expected));
	}

	#[test]
	fn json_span() {
		let pointer = "/event_duration";
		let context = serde_json::json!({
			"event_duration": "P1y2dT3h4m",
		});
		let span: jiff::Span = "P1y2dT3h4m".parse().unwrap();
		let expected = Primitive::Span(span).into();

		let val = lookup_json_pointer(pointer, &context).unwrap();
		let result = json_to_policy_expr(val, pointer, &context);
		assert_eq!(result, Ok(expected));
	}

	#[test]
	/// LookupJsonPointers writes to `value` in the JsonPointer Expr.
	fn toplevel_bool_set() {
		let expr = Expr::JsonPointer(JsonPointer {
			pointer: "".to_owned(),
			value: None,
		});
		let val = serde_json::json!(true);
		let expected = Expr::JsonPointer(JsonPointer {
			pointer: "".to_owned(),
			value: Some(Box::new(Primitive::Bool(true).into())),
		});

		let result = LookupJsonPointers::with_context(&val).visit_expr(expr);
		assert_eq!(result, Ok(expected))
	}

	#[test]
	/// LookupJsonPointers writes to `value` in the JsonPointer Expr.
	fn toplevel_f64_set() {
		let expr = Expr::JsonPointer(JsonPointer {
			pointer: "".to_owned(),
			value: None,
		});
		let val = serde_json::json!(1.23);
		let expected = Expr::JsonPointer(JsonPointer {
			pointer: "".to_owned(),
			value: Some(Box::new(Primitive::Float(F64::new(1.23).unwrap()).into())),
		});

		let result = LookupJsonPointers::with_context(&val).visit_expr(expr);
		assert_eq!(result, Ok(expected))
	}

	#[test]
	fn error_lookup_failed() {
		// Note spelling
		let expr = json_ptr("/alpa");
		let context = serde_json::json!({
			"alpha": 4.5,
		});
		let result = LookupJsonPointers::with_context(&context).visit_expr(expr);
		assert_eq!(
			result,
			Err(Error::JSONPointerLookupFailed {
				pointer: "/alpa".into(),
				context
			})
		);
	}

	#[test]
	fn error_unrepresentable_string() {
		let expr = json_ptr("/str");
		let context = serde_json::json!({
			"str": "Hello World!",
		});
		let result = LookupJsonPointers::with_context(&context).visit_expr(expr);
		assert_eq!(
			result,
			Err(Error::JSONPointerUnrepresentableType {
				json_type: error::UnrepresentableJSONType::JSONString,
				pointer: "/str".into(),
				value: context.get("str").unwrap().clone(),
				context,
			})
		);
	}

	#[test]
	fn error_unrepresentable_object() {
		let expr = json_ptr("/obj");
		let context = serde_json::json!({
			"obj": {
				"a": 4.5,
				"b": true,
			}
		});
		let result = LookupJsonPointers::with_context(&context).visit_expr(expr);
		assert_eq!(
			result,
			Err(Error::JSONPointerUnrepresentableType {
				json_type: error::UnrepresentableJSONType::JSONObject,
				pointer: "/obj".into(),
				value: context.get("obj").unwrap().clone(),
				context,
			})
		);
	}

	#[test]
	fn error_unrepresentable_array_nonprimitive() {
		let expr = json_ptr("/array");
		let context = serde_json::json!({
			"array": [0, [5, 10], 100],
		});
		let result = LookupJsonPointers::with_context(&context).visit_expr(expr);
		assert_eq!(
			result,
			Err(Error::JSONPointerUnrepresentableType {
				json_type: error::UnrepresentableJSONType::NonPrimitiveInArray,
				pointer: "/array".into(),
				value: serde_json::json!([5, 10]),
				context,
			})
		);
	}
}
