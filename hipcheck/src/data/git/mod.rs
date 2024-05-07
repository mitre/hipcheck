// SPDX-License-Identifier: Apache-2.0

mod data;
pub mod parse;
mod query;

pub use data::*;
use parse::*;
pub use query::*;

use crate::context::Context as _;
pub use crate::data::git_command::*;
use crate::error::Result;
use std::path::Path;

pub fn get_git_version() -> Result<String> {
	let raw_output = GitCommand::new_repo(["--version"])?
		.output()
		.context("git version command failed")?;
	log::debug!("get_git_version [raw_output='{}']", raw_output);
	Ok(raw_output)
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

pub fn get_commits_for_file(repo_path: &Path, file: &str) -> Result<String> {
	log::debug!("getting commits for file [file = '{}']", file);

	let output = GitCommand::for_repo(
		repo_path,
		["log", "--follow", "--pretty=tformat:%H%n", "--", file],
	)?
	.output()
	.context("git log hash command failed")?;

	Ok(output)
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::command_util::DependentProgram;

	#[test]
	#[ignore = "can't guarantee availability of Git"]
	fn parse_version() {
		let version = get_git_version().unwrap();
		DependentProgram::Git.check_version(&version).unwrap();
	}

	#[test]
	#[ignore = "can't guarantee availability of Git"]
	fn check_version_windows() {
		DependentProgram::Git
			.check_version("git version 2.24.0.windows.0")
			.unwrap();
	}

	#[test]
	#[ignore = "can't guarantee availability of Git"]
	fn check_version_macos() {
		DependentProgram::Git
			.check_version("git version 2.30.1 (Apple Git-130)")
			.unwrap();
	}
}
