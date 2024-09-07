// SPDX-License-Identifier: Apache-2.0

use crate::error::{Context as _, Result};
use command::ESLintCommand;
use data::ESLintReports;
use std::{ffi::OsStr, path::Path};

pub mod command;
pub mod data;

// rustfmt insists on splitting the args into separate lines,
// reducing readability
#[rustfmt::skip]
pub fn get_eslint_reports(path: &Path, version: String) -> Result<ESLintReports> {
	// https://eslint.org/docs/user-guide/command-line-interface
	let output = ESLintCommand::generic([
		OsStr::new("--format"), OsStr::new("json"),
		OsStr::new("--no-eslintrc"),
		OsStr::new("--rule"), OsStr::new(r#"{"no-eval": "error"}"#),
		OsStr::new("--rule"), OsStr::new(r#"{"no-implied-eval": "error"}"#),
		path.as_os_str(),
	], version)?
	.output()?;

	let reports = serde_json::from_str(&output)
		.context("Error parsing JSON output from ESLint")?;
	Ok(reports)
}

#[cfg(test)]
mod test {
	use super::*;

	use crate::command_util::DependentProgram;
	use std::{fs::File, io::Write};
	use tempfile::tempdir;

	#[test]
	#[ignore = "can't guarantee availability of ESLint"]
	fn check_version() {
		let version = "v7.31.0".to_string();
		DependentProgram::EsLint.check_version(&version).unwrap();
	}

	#[test]
	#[ignore = "can't guarantee availability of ESLint"]
	#[should_panic]
	fn software_not_installed() {
		let version = "'eslint' it not recognized as an internal or external command, operable program or batch file.".to_string();
		DependentProgram::EsLint.check_version(&version).unwrap();
	}

	#[test]
	#[ignore = "can't guarantee availability of ESLint"]
	fn run_eslint_basic() {
		let dir = tempdir().unwrap();
		let new_js_file = dir.path().join("bad.js");
		let mut f = File::create(new_js_file).unwrap();
		f.write_all(
			br#"
			function bad_function() {
				eval('code');
			}
		"#,
		)
		.unwrap();
		drop(f);

		let reports = get_eslint_reports(dir.path(), DependentProgram::EsLint.to_string()).unwrap();
		dbg!(&reports);
		assert_eq!(reports.len(), 1);
		assert!(reports[0].file_path.ends_with("bad.js"));
		assert_eq!(reports[0].messages.len(), 1);
		assert_eq!(reports[0].messages[0].rule_id, "no-eval");
		assert!(reports[0].source.is_some());
		assert!(reports[0].source.as_ref().unwrap().contains("bad_function"));
		assert!(reports[0].source.as_ref().unwrap().contains("eval"));
	}
}
