// SPDX-License-Identifier: Apache-2.0

mod data;
pub mod parse;
mod query;

pub use data::*;
pub use hc_git_command::*;
pub use query::*;

use crate::parse::*;
use hc_common::context::Context as _;
use hc_common::{
	error::{Error, Result},
	log,
};
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

pub fn get_last_commit(repo: &Path) -> Result<RawCommit> {
	let mut raw_commits = get_commits(repo)?;

	if raw_commits.is_empty() {
		Err(Error::msg("no commits"))
	} else {
		Ok(raw_commits.remove(0))
	}
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
	use hc_command_util::DependentProgram;

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
