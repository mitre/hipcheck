// SPDX-License-Identifier: Apache-2.0

use crate::data::*;
use crate::parse::*;
use crate::util::command::log_git_args;
// use crate::{
// 	error::{Context as _, Result},
// 	anyhow,
// };

use anyhow::{anyhow, Context as _, Result};
use std::{
	convert::AsRef, ffi::OsStr, iter::IntoIterator, ops::Not as _, path::Path, process::Command,
};

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
				Err(anyhow!("(from git) {} [{}]", msg.trim(), output.status))
			}
			_ => Err(anyhow!("git failed [{}]", output.status)),
		}
	}
}

pub fn get_commits(repo: &Path) -> Result<Vec<RawCommit>> {
	let raw_output = GitCommand::for_repo(
		repo,
		[
			"--no-pager",
			"log",
			"--no-merges",
			"--date=iso-strict",
			"--pretty=tformat:%H%n%aN%n%aE%n%ad%n%cN%n%cE%n%cd%n%GS%n%GK%n",
		],
	)?
	.output()
	.context("git log command failed")?;

	git_log(&raw_output)
}

pub fn get_commits_from_date(repo: &Path, date: &str) -> Result<Vec<RawCommit>> {
	let since_date = format!("--since='{} month ago'", date);
	let msg = format!("git log from date {} command failed", &date);
	let raw_output = GitCommand::for_repo(
		repo,
		[
			"--no-pager",
			"log",
			"--no-merges",
			"--date=iso-strict",
			"--pretty=tformat:%H%n%aN%n%aE%n%ad%n%cN%n%cE%n%cd%n%GS%n%GK%n",
			"--all",
			&since_date,
		],
	)?
	.output()
	.context(msg)?;

	git_log(&raw_output)
}

pub fn get_diffs(repo: &Path) -> Result<Vec<Diff>> {
	let output = GitCommand::for_repo(
		repo,
		[
			"--no-pager",
			"log",
			"--no-merges",
			"--numstat",
			"--pretty=tformat:",
			"-U0",
		],
	)?
	.output()
	.context("git diff command failed")?;

	git_diff(&output)
}
