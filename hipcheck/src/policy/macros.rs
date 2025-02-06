// SPDX-License-Identifier: Apache-2.0

use crate::{hc_error, Result};
use hipcheck_kdl::kdl::{KdlEntry, KdlValue};
use std::{path::Path, sync::LazyLock};

use pathbuf::pathbuf;

use regex::Regex;

static MACRO_REGEX: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r"#([a-z]{2,})(?:\(([^)]*)\)){0,1}").unwrap());

/// Expects a non-None opt_var of format `"<PATH>"`. Parses to a Path object and appends to parent
/// of the PolicyFile's path so that we can specify file paths as relative to the PolicyFile.
fn rel(opt_var: Option<&str>, file_path: &Path) -> Result<String> {
	let Some(var) = opt_var else {
		return Err(hc_error!("#rel macro expects an argument"));
	};

	// Parse `"<PATH"` to a KdlValue and extract contained string
	let entry = KdlEntry::parse(var).map_err(|e| hc_error!("{}", e))?;
	let KdlValue::String(s) = entry.value() else {
		return Err(hc_error!("Content of #rel macro must be a string!"));
	};

	// Dir of policy file + relative path parsed above
	let new_path = pathbuf![file_path.parent().unwrap(), s];

	let path_node = format!("\"{}\"", new_path.to_string_lossy().into_owned())
		// @Note - necessary because `to_string_lossy()` will insert backslashes on windows and
		// the KDL parser will complain
		.replace("\\", "/");

	Ok(path_node)
}

/// Expects a non-None opt_var of format `"<ENV_VAR>"`. Parses to an environment variable name on
/// the current system and resolves to the value of that env var or returns an error if not found.
fn env(opt_var: Option<&str>) -> Result<String> {
	let Some(var) = opt_var else {
		return Err(hc_error!("#env macro expects an argument"));
	};

	// Parse `"<PATH"` to a KdlValue and extract contained string
	let entry = KdlEntry::parse(var).map_err(|e| hc_error!("{}", e))?;
	let KdlValue::String(s) = entry.value() else {
		return Err(hc_error!("Content of #env macro must be a string!"));
	};

	let val = std::env::var(s)
		.map_err(|_| hc_error!("#env macro failed to resolve '{}' to a value", s))?;

	Ok(format!("\"{val}\""))
}

pub fn preprocess_policy_file(s: &str, file_path: &Path) -> Result<String> {
	let mut s = s.to_owned();
	// @Note - continues working until all macros resolved. If a macro returns another
	// macro continually, this will loop infinitely.
	while let Some(caps) = MACRO_REGEX.captures(s.as_str()) {
		let full = caps.get(0).unwrap(); // the full string
		let macro_name = caps.get(1).unwrap(); // the name of the macro
		let opt_parens = caps.get(2); // optional value in parentheses

		log::debug!("Handling macro: {}", macro_name.as_str());
		let opt_var = opt_parens.map(|x| x.as_str());

		// Call the right macro function given the `macro_name`, and get the string
		// to replace `full` with.
		let replace = match macro_name.as_str() {
			"rel" => rel(opt_var, file_path)?,
			"env" => env(opt_var)?,
			other => {
				return Err(hc_error!("Unknown policy file macro name '{}'", other));
			}
		};

		// Replace the character range specified by `full`, thus removing the macro
		s.replace_range(full.start()..full.end(), replace.as_str());
	}

	Ok(s.to_owned())
}
