// SPDX-License-Identifier: Apache-2.0

use hc_common::context::Context as _;
use hc_common::{
	command_util::{log_args, DependentProgram},
	error::{Error, Result},
	hc_error, pathbuf, serde_json,
};
use serde::{self, Deserialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::convert::AsRef;
use std::ffi::OsStr;
use std::fs;
use std::iter::IntoIterator;
use std::ops::Not as _;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn generate_module_model(repo_dir: &Path, module_deps: &Path) -> Result<Vec<RawModule>> {
	let root = detect_npm_package_root(&pathbuf![repo_dir, "package.json"])?;
	if !pathbuf![&repo_dir, &root].exists() {
		return Err(Error::msg(
			"Unable to identify module structure of repository code",
		));
	}
	let output = ModuleDepsCommand::for_path(repo_dir, module_deps, &[root])?.output()?;

	let raw_modules: Vec<RawModule> =
		serde_json::from_str(&output).context("failed to parse module-deps output")?;

	Ok(raw_modules)
}

fn detect_npm_package_root(pkg_file: &Path) -> Result<PathBuf> {
	let file_contents = fs::read_to_string(pkg_file);
	match file_contents {
		Ok(contents) => {
			let json: JsonValue = serde_json::from_str(&contents)?;
			let path = json
				.pointer("/exports")
				.or_else(|| json.pointer("/main"))
				.or_else(|| json.pointer("/browser"))
				.and_then(JsonValue::as_str)
				.unwrap_or("index.js");

			Ok(PathBuf::from(path))
		}
		Err(..) => Err(hc_error!(
			"package.json file not found; file missing or repo does not use Node.js modules"
		)),
	}
}

#[derive(Debug, Deserialize)]
#[serde(crate = "self::serde")]
pub struct RawModule {
	pub file: String,
	pub entry: bool,
	pub expose: Option<String>,
	pub deps: HashMap<String, String>,
}

#[derive(Debug)]
pub struct ModuleDepsCommand {
	command: Command,
}

impl ModuleDepsCommand {
	pub fn for_path<I, S>(
		pkg_path: &Path,
		module_deps_path: &Path,
		args: I,
	) -> Result<ModuleDepsCommand>
	where
		I: IntoIterator<Item = S> + Copy,
		S: AsRef<OsStr>,
	{
		ModuleDepsCommand::internal(Some(pkg_path), module_deps_path, args)
	}

	fn internal<I, S>(
		pkg_path: Option<&Path>,
		module_deps_path: &Path,
		args: I,
	) -> Result<ModuleDepsCommand>
	where
		I: IntoIterator<Item = S> + Copy,
		S: AsRef<OsStr>,
	{
		let path = module_deps_path.display().to_string();
		log_args(&path, args, DependentProgram::ModuleDeps);

		let mut command = Command::new(module_deps_path);
		command.args(args);

		// Set the path if necessary
		if let Some(pkg_path) = pkg_path {
			command.current_dir(pkg_path);
		}

		Ok(ModuleDepsCommand { command })
	}

	fn output(&mut self) -> Result<String> {
		let output = self.command.output()?;

		if output.status.success() {
			let output_text = String::from_utf8_lossy(&output.stdout).to_string();
			return Ok(output_text);
		}

		match String::from_utf8(output.stderr) {
			Ok(msg) if msg.is_empty().not() => Err(hc_error!(
				"(from module-deps) {} [{}]",
				msg.trim(),
				output.status
			)),
			_ => Err(hc_error!("module-deps failed [{}]", output.status)),
		}
	}
}

// #[cfg(test)]
// mod tests {
// 	use super::*;
// 	use std::env::current_exe;
// 	use std::io::Result as IoResult;
// 	use std::path::PathBuf;

// 	#[test]
// 	fn can_run_module_deps() {
// 		let path = get_testpkg_path().unwrap();
// 		if let Ok(mut command) = ModuleDepsCommand::for_path(&path, ,&["main.js"]) {
// 			let _ = command.output().unwrap();
// 		}
// 	}

// 	// Return the absolute path to testpkg
// 	fn get_testpkg_path() -> IoResult<PathBuf> {
// 		let mut path = current_exe()?;
// 		for _ in 0..4 {
// 			path.pop();
// 		}
// 		path.push("libs/hc_data/src/modules/testpkg");

// 		Ok(path)
// 	}
// }
