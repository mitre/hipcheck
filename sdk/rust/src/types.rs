// SPDX-License-Identifier: Apache-2.0

use std::{fmt::Debug, str::FromStr};

use jiff::{Span, Zoned};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use typify_macro::import_types;

import_types!(
	schema = "../schema/hipcheck_target_schema.json",
	derives = [schemars::JsonSchema],
	convert = {
		{
			type = "string",
			format = "uri",
		} = url::Url,
	}
);

/// Opaque type that holds data of a type that requires special type annotation
/// in JSON to parse correctly.
///
/// Currently, the following types are supported:
/// - `jiff::Zoned`
/// - `Jiff::Span`
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct AnnotatedJSONValue(AnnotatedJSONValueInner);

impl AnnotatedJSONValue {
	/// Create a new AnnotatedJSONValue from a `jiff::Span`
	pub fn from_span(span: jiff::Span) -> Self {
		Self(AnnotatedJSONValueInner::Duration(span.to_string()))
	}

	/// Convert, if possible, the inner type to `jiff::Span`
	pub fn to_span(&self) -> Option<Span> {
		match &self.0 {
			AnnotatedJSONValueInner::DateTime(_) => None,
			// unwrap is safe because the only way to construct a DateTime variant is from a valid
			// `jiff::Span`
			AnnotatedJSONValueInner::Duration(span) => Some(Span::from_str(span).unwrap()),
		}
	}

	/// Create a new AnnotatedJSONValue from a `jiff::Zoned`
	pub fn from_datetime(duration: jiff::Zoned) -> Self {
		Self(AnnotatedJSONValueInner::DateTime(duration.to_string()))
	}

	/// Convert, if possible, the inner type to `jiff::Zoned`
	pub fn to_datetime(&self) -> Option<Zoned> {
		match &self.0 {
			// unwrap is safe because the only way to construct a DateTime variant is from a valid
			// `jiff::Zoned`
			AnnotatedJSONValueInner::DateTime(datetime) => Some(Zoned::from_str(datetime).unwrap()),
			AnnotatedJSONValueInner::Duration(_) => None,
		}
	}
}

/// This type is purposely opaque to the user to prevent accessing inner fields of the enum and to
/// prevent creating invalid variants
///
/// The data will be a string called "data", and the format of the data will be
/// in "format".
/// Format names taken from JSON Schema.
/// Spec as of "Draft 2020-12":
/// https://json-schema.org/draft/2020-12/json-schema-validation#name-defined-formats
/// Prettier rendered docs of latest version:
/// https://json-schema.org/understanding-json-schema/reference/string#built-in-formats
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "format", content = "data")]
#[serde(rename_all = "lowercase")]
enum AnnotatedJSONValueInner {
	/// Contains a string representing a jiff::Zoned
	/// According to JSON Schema, "format" = "date-time" refers to a timestamp
	/// encoded as specified by RFC 3339 (a subset of ISO 8601):
	/// https://datatracker.ietf.org/doc/html/rfc3339#section-5.6
	#[serde(rename = "date-time")]
	DateTime(String),
	/// Contains a string representing a jiff::Span
	/// According to JSON Schema, "format" = "duration" refers to a time duration
	/// encoded as specified by ISO 8601's ABNF:
	/// https://datatracker.ietf.org/doc/html/rfc3339#appendix-A
	#[serde(rename = "duration")]
	Duration(String),
}
