// SPDX-License-Identifier: Apache-2.0
use std::{convert::AsRef, env, ffi::OsStr, iter::IntoIterator};

/// Print command line args as well as commands and args for git commands
pub fn log_git_args<I, S>(repo_path: &str, args: I, git_path: &str)
where
	I: IntoIterator<Item = S> + Copy,
	S: AsRef<OsStr>,
{
	log::debug!("logging git CLI args");

	for arg in env::args() {
		log::debug!("git CLI environment arg [arg='{}']", arg);
	}

	log::debug!("git CLI executable location [path='{}']", git_path);

	log::debug!("git CLI repository location [path='{}']", repo_path);

	log_each_git_arg(args);

	log::debug!("done logging git CLI args");
}

pub fn log_each_git_arg<I, S>(args: I)
where
	I: IntoIterator<Item = S>,
	S: AsRef<OsStr>,
{
	for (index, val) in args.into_iter().enumerate() {
		let arg_val = val
			.as_ref()
			.to_str()
			.unwrap_or("argument for command could not be logged.");

		log::debug!("git CLI argument [name='{}', value='{}']", index, arg_val);
	}
}
