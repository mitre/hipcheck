// SPDX-License-Identifier: Apache-2.0

use crate::{
	command_util::{log_args, DependentProgram},
	error::{Context, Result},
	hc_error,
	util::fs as file,
};
use pathbuf::pathbuf;
use serde::Deserialize;
use std::{
	collections::HashMap,
	convert::AsRef,
	ffi::OsStr,
	iter::IntoIterator,
	ops::Not,
	path::{Path, PathBuf},
	process::{Child, Command, Stdio},
};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PackageFile {
	pub main: String,
}

/// Read the contents of `package.json`.
/// Assumes that it exists at the toplevel of the repository.
pub fn get_package_file(repo: &Path) -> Result<PackageFile> {
	let package_file = pathbuf!(repo, "package.json");
	file::read_json(package_file)
}

// For working with dependencies, I think the flow would look like this:
//
// 1. Check if a `package-shrinkwrap.json` file is available. If it is,
//    use it instead of a `package-lock.json` file.
// 2. Check if a `package-lock.json` file is available. If it isn't,
//    generate one and use it. (concern here: because we may be dealing
//    with malicious inputs, we want to make sure we aren't accidentally
//    hitting scripts in malicious dependencies with the `preshrinkwrap`
//    script event or other events like it). Looks like `--ignore-scripts`
//    is the way to do that, but we want to be sure that won't change the
//    resolution of the package hierarchy.
// 3. Once we have a `package-shrinkwrap.json` or a `package-lock.json`,
//    we have the names of all dependencies, including transitive
//    dependencies. So we can then extract them and check them for typos.

pub fn get_dependencies(repo: &Path, version: String) -> Result<Vec<String>> {
	let package_lock_file = get_package_lock_file(repo, version)?;
	Ok(package_lock_file.dependencies())
}

/// Get the package file, ensuring that it exists.
/// Prefer the shrinkwrap file. If it doesn't exist,
/// use the lock file, which may need to be created
/// if nothing exists at all.
///
/// In creating the lock file, we make sure to ignore
/// scripts, otherwise malicious dependencies may end up
/// owning the host running typospotter by executing some
/// preinstall scripts.
fn get_package_lock_file(package_dir: &Path, version: String) -> Result<PackageLockFile> {
	let package_lock_file = get_package_lock_file_name(package_dir, version)?;
	file::read_json(package_lock_file)
}

fn get_package_lock_file_name(package_dir: &Path, version: String) -> Result<PathBuf> {
	let shrinkwrap_file = pathbuf!(package_dir, "npm-shrinkwrap.json");

	if shrinkwrap_file.exists() {
		return Ok(shrinkwrap_file);
	}

	let lock_file = pathbuf!(package_dir, "package-lock.json");

	if !lock_file.exists() {
		generate_package_lock_file(package_dir, version)?;
	}

	Ok(lock_file)
}

pub fn get_npm_version() -> Result<String> {
	NpmCommand::version(["--version"])?.output()
}

fn generate_package_lock_file(package_dir: &Path, version: String) -> Result<()> {
	log::debug!("generating lock file [path={}]", package_dir.display());

	NpmCommand::for_package(
		package_dir,
		version,
		[
			// Override any `.npmrc` configuration which would otherwise turn off
			// lockfile production. We need the lockfile to proceed.
			"--package-lock=true",
			"install",
			// This may break some builds, so it's a trade-off. It helps protect us
			// from remote code execution from untrusted package dependencies.
			"--ignore-scripts",
			// The rest just speed things up, and help protect users' confidentiality.
			"--no-audit",
			"--no-bin-links",
			"--no-fund",
		],
	)?
	.spawn()
	.context("failed to spawn 'npm install'")?
	.wait()
	.context("failed to run 'npm install'")?;

	Ok(())
}

#[derive(Deserialize)]
struct PackageLockFile {
	dependencies: Option<HashMap<String, Box<Dependency>>>,
}

impl PackageLockFile {
	fn dependencies(&self) -> Vec<String> {
		match &self.dependencies {
			None => vec![],
			Some(deps) => deps
				.iter()
				.flat_map(|(name, detail)| resolve_deps(name, detail))
				.collect(),
		}
	}
}

/// Recursively resolves dependencies in a package file using depth-first search.
fn resolve_deps(name: &str, detail: &Dependency) -> Vec<String> {
	let mut results = vec![name.to_owned()];

	match &detail.dependencies {
		None => results,
		Some(deps) => {
			// If there are more dependencies, add them to the list.
			for (name, detail) in deps {
				results.extend(resolve_deps(name, detail));
			}

			results
		}
	}
}

#[derive(Deserialize)]
struct Dependency {
	dependencies: Option<HashMap<String, Box<Dependency>>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Out {
	Null,
	Piped,
}

#[derive(Debug)]
pub struct NpmCommand {
	command: Command,
}

impl NpmCommand {
	pub fn for_package<I, S>(pkg_path: &Path, version: String, args: I) -> Result<NpmCommand>
	where
		I: IntoIterator<Item = S> + Copy,
		S: AsRef<OsStr>,
	{
		DependentProgram::Npm.check_version(&version)?;

		log::debug!(
			"minimum version in use [min='{}', version='{}']",
			DependentProgram::Npm,
			&version
		);

		NpmCommand::internal(Some(pkg_path), Out::Null, args)
	}

	pub fn version<I, S>(args: I) -> Result<NpmCommand>
	where
		I: IntoIterator<Item = S> + Copy,
		S: AsRef<OsStr>,
	{
		NpmCommand::internal(None, Out::Piped, args)
	}

	fn internal<I, S>(pkg_path: Option<&Path>, output: Out, args: I) -> Result<NpmCommand>
	where
		I: IntoIterator<Item = S> + Copy,
		S: AsRef<OsStr>,
	{
		// Init the command.
		let npm_path = which::which("npm")
			.context("failed to run npm, make sure npm is in the PATH and installed")?;

		let mut command = Command::new(&npm_path);
		command.args(args);

		log_args(&npm_path.display().to_string(), args, DependentProgram::Npm);

		// Set the path if necessary
		if let Some(pkg_path) = pkg_path {
			command.current_dir(pkg_path);
		}

		match output {
			Out::Null => {
				command.stdout(Stdio::null());
				command.stderr(Stdio::null());
			}
			Out::Piped => {
				command.stdout(Stdio::piped());
				command.stderr(Stdio::piped());
			}
		}

		Ok(NpmCommand { command })
	}

	pub fn spawn(&mut self) -> Result<Child> {
		Ok(self.command.spawn()?)
	}

	pub fn output(&mut self) -> Result<String> {
		let output = self.command.output()?;
		let output_text = String::from_utf8_lossy(&output.stdout).to_string();
		if output.status.success() {
			return Ok(output_text);
		}
		log::debug!(
			"{} output_text [output_text='{}']",
			DependentProgram::Npm,
			output_text
		);

		match String::from_utf8(output.stderr) {
			Ok(msg) if msg.is_empty().not() => Err(hc_error!(
				"(from {}) {} [{}]",
				DependentProgram::Npm,
				msg.trim(),
				output.status
			)),
			_ => Err(hc_error!(
				"{} failed [{}]",
				DependentProgram::Npm,
				output.status
			)),
		}
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::command_util::DependentProgram;

	#[test]
	#[ignore = "can't guarantee availability of NPM"]
	fn parse_version() {
		let version = get_npm_version().unwrap();
		DependentProgram::Npm.check_version(&version).unwrap();
	}

	#[test]
	#[ignore = "can't guarantee availability of NPM"]
	fn check_version() {
		let version = "7.12.1".to_string();
		DependentProgram::Npm.check_version(&version).unwrap();
	}
}
