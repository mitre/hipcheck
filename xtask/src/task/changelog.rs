// SPDX-License-Identifier: Apache-2.0

use crate::ChangelogArgs;
use anyhow::{Context, Result};
use log::LevelFilter;
use xshell::{Shell, cmd};

/// Execute the changelog task.
pub fn run(args: ChangelogArgs) -> Result<()> {
	which::which("git-cliff").context(
        "changelog requires `git-cliff` to be installed. Please install it with `cargo install git-cliff`.")?;

	let sh = Shell::new()?;

	// Get the root of the workspace, and make sure the shell is in it.
	// `git-cliff` expects to be run from the root of the workspace.
	let root = crate::workspace::root()?;
	sh.change_dir(root);

	// Warn the user about what version we're bumping to.
	//
	// Avoid running the extra external command if we won't see the result.
	if log::max_level() >= LevelFilter::Warn {
		let new_version = {
			let full = cmd!(sh, "git cliff --bumped-version").read()?;
			full.strip_prefix("hipcheck-").unwrap_or(&full).to_owned()
		};

		log::warn!(
			"bumping to {}; this may not be the version you want",
			new_version
		);
	}

	// We don't overwrite the CHANGELOG.md file, and instead by default
	// write to a different output file.
	let output = "CHANGELOG-NEXT.md";

	// Only include the bump flag if requested by the user.
	let bump = args.bump.then_some("--bump");
	cmd!(sh, "git cliff {bump...} -o {output}")
		.quiet()
		.ignore_stdout()
		.ignore_stderr()
		.run()?;

	log::warn!("finished; check {} to proceed", output);
	Ok(())
}
