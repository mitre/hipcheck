// SPDX-License-Identifier: Apache-2.0

use std::ffi::OsStr;
use std::process::Command;

use hc_command_util;
use hc_command_util::{log_args, DependentProgram};
use hc_common::context::Context as _;
use hc_common::{error::Result, hc_error, log, which};

#[derive(Debug)]
pub struct ESLintCommand {
	command: Command,
}

impl ESLintCommand {
	pub fn generic<I, S>(args: I, version: String) -> Result<Self>
	where
		I: IntoIterator<Item = S> + Copy,
		S: AsRef<OsStr>,
	{
		let right_version = DependentProgram::EsLint
			.check_version(&version)
			.context("check_min_version command failed")?;
		if right_version {
			log::debug!(
				"minimum version in use [min='{}', version='{}']",
				DependentProgram::EsLint,
				&right_version
			);
		}
		ESLintCommand::internal(args)
	}

	pub fn internal<I, S>(args: I) -> Result<Self>
	where
		I: IntoIterator<Item = S> + Copy,
		S: AsRef<OsStr>,
	{
		let eslint_path = which::which("eslint").context("eslint not found on the PATH")?;
		let path = eslint_path.display().to_string();
		log_args(&path, args, DependentProgram::EsLint);

		let mut command = Command::new(&eslint_path);
		command.args(args);

		Ok(Self { command })
	}

	pub fn output(&mut self) -> Result<String> {
		let output = self.command.output()?;

		// ESLint exits with code 1 when linting resulted in issues,
		// but there were no configuration problems preventing operation.
		// https://eslint.org/docs/user-guide/command-line-interface#exit-codes
		if let Some(0 | 1) = output.status.code() {
			let output_text = String::from_utf8_lossy(&output.stdout).to_string();
			return Ok(output_text);
		}

		let msg =
			String::from_utf8(output.stderr).context("failed to decode eslint error as UTF-8")?;

		if msg.is_empty() {
			Err(hc_error!(
				"eslint failed (no error message) [{}]",
				output.status
			))
		} else {
			Err(hc_error!("eslint: {} [{}]", msg.trim(), output.status))
		}
	}
}
