// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, Context as _, Result};
use regex::Regex;
use semver::Version;
use std::{convert::AsRef, env, ffi::OsStr, iter::IntoIterator};

// Minimum NPM version allowed
const MIN_VERSION: &str = "6.0.0";

pub fn check_version(version: &str) -> Result<bool> {
	let version = parse_version(version)?;
	let min_version = &min_version();

	if &version >= min_version {
		Ok(true)
	} else {
		Err(anyhow!(
			"npm version is {}; must be >= {}",
			version,
			min_version
		))
	}
}

pub fn min_version() -> Version {
	// Panic: Safe to unwrap because the const MIN_VERSION is a valid version
	Version::parse(MIN_VERSION).unwrap()
}

fn parse_version(version: &str) -> Result<Version> {
	// Typical version strings, at least on MacOS:
	//
	// - npm: `6.14.15`

	let re = Regex::new(r"(\d+\.\d+\.\d+)").context("failed to build version regex")?;

	let cap = re
		.captures(version)
		.ok_or_else(|| anyhow!("failed to capture npm version output"))?;

	let version = cap
		.get(1)
		.ok_or_else(|| anyhow!("failed to find a npm version"))?
		.as_str();

	log::debug!("{} version detected [version='npm']", version);

	Ok(Version::parse(version)?)
}

/// print command line args as well as commands and args for npm and other non git commands
pub fn log_args<I, S>(command_path: &str, args: I)
where
	I: IntoIterator<Item = S> + Copy,
	S: AsRef<OsStr>,
{
	log::debug!("logging npm CLI args");

	// https://doc.rust-lang.org/std/env/fn.args.html
	for arg in env::args() {
		log::debug!("npm CLI environment arg [arg='{}']", arg);
	}

	log::debug!("npm CLI executable location [path='{}']", command_path);

	log_each_arg(args);

	log::debug!("done logging npm CLI args");
}

pub fn log_each_arg<I, S>(args: I)
where
	I: IntoIterator<Item = S>,
	S: AsRef<OsStr>,
{
	for (index, val) in args.into_iter().enumerate() {
		let arg_val = val
			.as_ref()
			.to_str()
			.unwrap_or("argument for command could not be logged.");

		log::debug!("npm CLI argument [name='{}', value='{}']", index, arg_val);
	}
}
