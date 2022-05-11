// SPDX-License-Identifier: Apache-2.0

use hc_common::{log, semver::Version};
use hc_error::{hc_error, Context as _, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;
use std::convert::AsRef;
use std::env;
use std::ffi::OsStr;
use std::fmt::{self, Display, Formatter};
use std::iter::IntoIterator;

use DependentProgram::*;

type VersionMap = HashMap<DependentProgram, Version>;

static MIN_VERSIONS: Lazy<VersionMap> = Lazy::new(|| {
	fn insert_version(hm: &mut VersionMap, program: DependentProgram) {
		// SAFETY: The versions in `min_version_str` are known to be valid.
		hm.insert(program, Version::parse(program.min_version_str()).unwrap());
	}

	let mut versions = HashMap::new();
	insert_version(&mut versions, EsLint);
	insert_version(&mut versions, Git);
	insert_version(&mut versions, Npm);
	insert_version(&mut versions, ModuleDeps);
	versions
});

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum DependentProgram {
	EsLint,
	Git,
	Npm,
	ModuleDeps,
}

impl DependentProgram {
	pub fn check_version(&self, version: &str) -> Result<bool> {
		let version = parse_version(*self, version)?;
		let min_version = self
			.min_version()
			.ok_or_else(|| hc_error!("failed to get min version for {}", self))?;

		if &version >= min_version {
			Ok(true)
		} else {
			Err(hc_error!(
				"{} version is {}; must be >= {}",
				self,
				version,
				min_version
			))
		}
	}

	pub fn min_version<'s, 'v>(&'s self) -> Option<&'v Version> {
		MIN_VERSIONS.get(self)
	}

	fn min_version_str(&self) -> &'static str {
		match self {
			// https://github.com/eslint/eslint/blob/main/CHANGELOG.md
			EsLint => "7.0.0",

			// https://github.com/git/git/search?q="flag-goes-here"+in%3Afile+filename%3A*.txt+path%3ADocumentation%2FRelNotes%2F
			Git => "2.14.0",

			// https://docs.npmjs.com/cli/v6/commands
			Npm => "6.0.0",

			// `module-deps` doesn't report a version number, so we just lie here.
			ModuleDeps => "0.0.0",
		}
	}
}

impl Display for DependentProgram {
	fn fmt(&self, f: &mut Formatter) -> fmt::Result {
		let name = match self {
			EsLint => "eslint",
			Git => "git",
			Npm => "npm",
			ModuleDeps => "module-deps",
		};

		write!(f, "{}", name)
	}
}

fn parse_version(program: DependentProgram, version: &str) -> Result<Version> {
	// Typical version strings, at least on MacOS:
	//
	// - git: `git version 2.30.1 (Apple Git-130)`
	// - eslint: `v7.32.0`
	// - npm: `6.14.15`

	let re = Regex::new(r"(\d+\.\d+\.\d+)").context("failed to build version regex")?;

	let cap = re
		.captures(version)
		.ok_or_else(|| hc_error!("failed to capture {} version output", program))?;

	let version = cap
		.get(1)
		.ok_or_else(|| hc_error!("failed to find a {} version", program))?
		.as_str();

	log::debug!("{} version detected [version='{}']", program, version);

	Ok(Version::parse(version)?)
}

/// Print command line args as well as commands and args for git commands
pub fn log_git_args<I, S>(repo_path: &str, args: I, git_path: &str)
where
	I: IntoIterator<Item = S> + Copy,
	S: AsRef<OsStr>,
{
	let program = Git;

	log::debug!("logging {} CLI args", program);

	for arg in env::args() {
		log::debug!("{} CLI environment arg [arg='{}']", program, arg);
	}

	log::debug!("{} CLI executable location [path='{}']", program, git_path);

	log::debug!("{} CLI repository location [path='{}']", program, repo_path);

	log_each_arg(args, program);

	log::debug!("done logging {} CLI args", DependentProgram::Git);
}

/// print command line args as well as commands and args for npm and other non git commands
pub fn log_args<I, S>(command_path: &str, args: I, program: DependentProgram)
where
	I: IntoIterator<Item = S> + Copy,
	S: AsRef<OsStr>,
{
	log::debug!("logging {} CLI args", &program);

	// https://doc.rust-lang.org/std/env/fn.args.html
	for arg in env::args() {
		log::debug!("{} CLI environment arg [arg='{}']", program, arg);
	}

	log::debug!(
		"{} CLI executable location [path='{}']",
		program,
		command_path
	);

	log_each_arg(args, program);

	log::debug!("done logging {} CLI args", &program);
}

pub fn log_each_arg<I, S>(args: I, program: DependentProgram)
where
	I: IntoIterator<Item = S>,
	S: AsRef<OsStr>,
{
	for (index, val) in args.into_iter().enumerate() {
		let arg_val = val
			.as_ref()
			.to_str()
			.unwrap_or("argument for command could not be logged.");

		log::debug!(
			"{} CLI argument [name='{}', value='{}']",
			program,
			index,
			arg_val
		);
	}
}
