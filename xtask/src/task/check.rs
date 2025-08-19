// SPDX-License-Identifier: Apache-2.0

use crate::{
	BuildArgs,
	build::{resolve_builder_args, resolve_packages},
	string::list_with_commas,
};
use anyhow::Result;
use itertools::Itertools;
use log::debug;
use std::collections::BTreeSet;
use xshell::{Shell, cmd};

/// Run the build command.
pub fn run(args: BuildArgs) -> Result<()> {
	debug!("check targeting: {}", list_with_commas(&args.pkg));

	let pkgs = args
		.pkg
		.into_iter()
		.flat_map(resolve_packages)
		.unique()
		.collect::<BTreeSet<_>>();

	let builder_args = resolve_builder_args(&pkgs, &args.profile, args.timings);

	debug!("checking packages: {}", list_with_commas(&pkgs));
	debug!("using profile: {}", args.profile);

	let sh = Shell::new()?;
	cmd!(sh, "cargo check {builder_args...}").run()?;

	Ok(())
}
