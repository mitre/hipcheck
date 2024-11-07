// SPDX-License-Identifier: Apache-2.0

typify_macro::import_types!(
	schema = "../schema/hipcheck_target_schema.json",
	derives = [schemars::JsonSchema],
	convert = {
		{
			type = "string",
			format = "uri",
		} = url::Url,
	   {
			type = "string",
			format = "date-time"
		} = jiff::Zoned,
		{
			type = "string",
			format = "duration"
		} = jiff::Span,
	}
);
