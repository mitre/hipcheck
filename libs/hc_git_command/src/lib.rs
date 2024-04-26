// SPDX-License-Identifier: Apache-2.0

use hc_command_util::log_git_args;
use hc_common::context::Context as _;
use hc_common::{error::Result, hc_error, which};
use std::convert::AsRef;
use std::ffi::OsStr;
use std::iter::IntoIterator;
use std::ops::Not as _;
use std::path::Path;
use std::process::Command;

#[derive(Debug)]
pub struct GitCommand {
	command: Command,
}

impl GitCommand {
	pub fn for_repo<I, S>(repo_path: &Path, args: I) -> Result<GitCommand>
	where
		I: IntoIterator<Item = S> + Copy,
		S: AsRef<OsStr>,
	{
		GitCommand::internal(Some(repo_path), args)
	}

	pub fn new_repo<I, S>(args: I) -> Result<GitCommand>
	where
		I: IntoIterator<Item = S> + Copy,
		S: AsRef<OsStr>,
	{
		GitCommand::internal(None, args)
	}

	fn internal<I, S>(repo_path: Option<&Path>, args: I) -> Result<GitCommand>
	where
		I: IntoIterator<Item = S> + Copy,
		S: AsRef<OsStr>,
	{
		// Init the command.
		let git_path = which::which("git").context("can't find git command")?;
		let no_repo_found = Path::new("no_repo_found");
		let repo = repo_path.unwrap_or(no_repo_found).display().to_string();
		let path = git_path.display().to_string();
		log_git_args(&repo, args, &path);
		let mut command = Command::new(&git_path);
		command.args(args);

		// Set the path if necessary
		if let Some(repo_path) = repo_path {
			command.current_dir(repo_path);
		}

		if cfg!(windows) {
			// this method is broken on Windows. See: https://github.com/rust-lang/rust/issues/31259
			//command.env_clear()
		} else {
			command.env_clear();
		};

		Ok(GitCommand { command })
	}

	pub fn output(&mut self) -> Result<String> {
		let output = self.command.output()?;

		if output.status.success() {
			let output_text = String::from_utf8_lossy(&output.stdout).to_string();
			return Ok(output_text);
		}

		match String::from_utf8(output.stderr) {
			Ok(msg) if msg.is_empty().not() => {
				Err(hc_error!("(from git) {} [{}]", msg.trim(), output.status))
			}
			_ => Err(hc_error!("git failed [{}]", output.status)),
		}
	}
}
