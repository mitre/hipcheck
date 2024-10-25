// SPDX-License-Identifier: Apache-2.0

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
