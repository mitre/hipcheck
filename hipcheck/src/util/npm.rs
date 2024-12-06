// SPDX-License-Identifier: Apache-2.0

use crate::{
	error::{Context as _, Result},
	hc_error,
	util::command::{log_args, DependentProgram},
};
use std::{
	ffi::OsStr,
	ops::Not as _,
	path::Path,
	process::{Command, Stdio},
};

pub fn get_npm_version() -> Result<String> {
	NpmCommand::version(["--version"])?.output()
}

#[derive(Debug)]
pub struct NpmCommand {
	command: Command,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Out {
	Piped,
}

impl NpmCommand {
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
			Out::Piped => {
				command.stdout(Stdio::piped());
				command.stderr(Stdio::piped());
			}
		}

		Ok(NpmCommand { command })
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
	use crate::util::command::DependentProgram;

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
