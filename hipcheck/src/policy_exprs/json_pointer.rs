// SPDX-License-Identifier: Apache-2.0

use crate::policy_exprs::error;
use crate::policy_exprs::error::Error;
use crate::policy_exprs::error::Result;
use crate::policy_exprs::expr::Expr;
use crate::policy_exprs::expr::Primitive;
use ordered_float::NotNan;
use regex::Captures;
use regex::Regex;
use regex::RegexBuilder;
use serde_json::Value;

/// Preprocess a Policy Expr source string by replacing JSON Pointer syntax with
/// values looked up from the `context` data.
pub(crate) fn process_json_pointers(raw_program: &str, context: &Value) -> Result<String> {
	let re = json_pointer_regex();
	let mut any_error: bool = false;
	let mut errors: Vec<Error> = Vec::new();
	let result = re.replace_all(raw_program, |caps: &Captures| {
		let pointer = &caps[1];
		let res = process_pointer(pointer, context);
		match res {
			Ok(expr) => expr,
			Err(e) => {
				any_error = true;
				errors.push(e);
				// Return a bogus string from the closure for Regex.replace_all to use.
				// The final string should never be used in that case.
				"ERROR".into()
			}
		}
	});

	if any_error {
		if errors.len() > 1 {
			Err(Error::MultipleErrors(errors))
		} else {
			Err(errors.remove(0))
		}
	} else {
		Ok(result.into_owned())
	}
}

/// Return the Regex used for parsing JSON pointers embedded in a Policy Expression.
/// Note that the initial $ is not captured.
/// A valid JSON Pointer must be either empty or start with '/', but this regex
/// still captures invalid syntax to provide better error handling.
fn json_pointer_regex() -> Regex {
	let pat = r"
		# JSON pointers embedded in policy expressions are signified by $.
		# It is not part of JSON pointer syntax, so it is not captured.
		\$
		(
			[
				/
				~
				_
				[:alnum:]
			]
			*
		)
	";
	// Panic safety: the regex is static and programmer-defined.
	// It is considered a programmer error if the regex syntax is invalid.
	RegexBuilder::new(pat)
		.ignore_whitespace(true)
		.build()
		.unwrap()
}

/// Lookup a single JSON Pointer reference and convert it to Policy Expr syntax,
/// if possible.
fn process_pointer(pointer: &str, context: &Value) -> Result<String> {
	let val = lookup_json_pointer(pointer, context)?;
	let expr = json_to_policy_expr(val, pointer, context)?;
	Ok(expr.to_string())
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
				pointer: pointer.to_owned(),
			});
		}
	}
	match context.pointer(pointer) {
		Some(val) => Ok(val),
		None => Err(Error::JSONPointerLookupFailed {
			pointer: pointer.to_owned(),
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
			Ok(Expr::Array(primitives))
		}
		// Strings cannot (currently) be represented in the Policy Expr language.
		Value::String(_) => Err(Error::JSONPointerUnrepresentableType {
			json_type: error::UnrepresentableJSONType::JSONString,
			pointer: pointer.to_owned(),
			value: val.clone(),
			context: context.clone(),
		}),
		Value::Object(_) => Err(Error::JSONPointerUnrepresentableType {
			json_type: error::UnrepresentableJSONType::JSONObject,
			pointer: pointer.to_owned(),
			value: val.clone(),
			context: context.clone(),
		}),
		Value::Null => Err(Error::JSONPointerUnrepresentableType {
			json_type: error::UnrepresentableJSONType::JSONNull,
			pointer: pointer.to_owned(),
			value: val.clone(),
			context: context.clone(),
		}),
	}
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
			pointer: pointer.to_owned(),
			value: v.clone(),
			context: context.clone(),
		}),
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use test_log::test;

	fn parse_json_pointer(src: &str) -> Option<(&str, &str)> {
		json_pointer_regex().captures(src).map(|caps| {
			let (whole, [cap]) = caps.extract();
			(whole, cap)
		})
	}

	#[test]
	fn parse_basic_slashes() {
		let src = "(eq 1 $/data/one)";
		let matches = parse_json_pointer(src);
		assert_eq!(matches, Some(("$/data/one", "/data/one")));
	}

	#[test]
	fn basic_float() {
		let program = "$";
		let context = serde_json::json!(2.3);
		let processed = process_json_pointers(program, &context).unwrap();
		assert_eq!(processed, "2.3");
	}

	#[test]
	fn basic_bool() {
		let program = "$";
		let context = serde_json::json!(true);
		let processed = process_json_pointers(program, &context).unwrap();
		assert_eq!(processed, "#t");
	}

	#[test]
	fn underscore() {
		let program = "(lte 0.05 $/pct_reviewed)";
		let context = serde_json::json!({
			"pct_reviewed": 0.15,
		});
		let processed = process_json_pointers(program, &context).unwrap();
		assert_eq!(processed, "(lte 0.05 0.15)");
	}

	#[test]
	fn multiple() {
		let program = "$/alpha $/bravo $/charlie";
		let context = serde_json::json!({
			"alpha": 4.5,
			"bravo": false,
			"charlie": [0, 1, 2, 3],
		});
		let processed = process_json_pointers(program, &context).unwrap();
		assert_eq!(processed, "4.5 #f [0 1 2 3]");
	}

	#[test]
	fn error_lookup_failed() {
		// Note spelling
		let program = "$/alpa";
		let context = serde_json::json!({
			"alpha": 4.5,
		});
		let result = process_json_pointers(program, &context);
		assert_eq!(
			result,
			Err(Error::JSONPointerLookupFailed {
				pointer: "/alpa".into(),
				context
			})
		);
	}

	#[test]
	fn error_invalid_syntax() {
		// Note missing '/' at beginning of pointer
		let program = "$alpha";
		let context = serde_json::json!({
			"alpha": 4.5,
		});
		let result = process_json_pointers(program, &context);
		assert_eq!(
			result,
			Err(Error::JSONPointerInvalidSyntax {
				pointer: "alpha".into()
			})
		);
	}

	#[test]
	fn multiple_errors() {
		let program = "$/alpa $/brave";
		let context = serde_json::json!({
			"alpha": 4.5,
			"bravo": false,
		});
		let result = process_json_pointers(program, &context);
		assert_eq!(
			result,
			Err(Error::MultipleErrors(vec![
				Error::JSONPointerLookupFailed {
					pointer: "/alpa".into(),
					context: context.clone(),
				},
				Error::JSONPointerLookupFailed {
					pointer: "/brave".into(),
					context: context.clone(),
				},
			]))
		);
	}

	#[test]
	fn error_unrepresentable_string() {
		let program = "$/str";
		let context = serde_json::json!({
			"str": "Hello World!",
		});
		let result = process_json_pointers(program, &context);
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
		let program = "$/obj";
		let context = serde_json::json!({
			"obj": {
				"a": 4.5,
				"b": true,
			}
		});
		let result = process_json_pointers(program, &context);
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
		let program = "$/array";
		let context = serde_json::json!({
			"array": [0, [5, 10], 100],
		});
		let result = process_json_pointers(program, &context);
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
