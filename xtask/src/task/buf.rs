// SPDX-License-Identifier: Apache-2.0

use crate::workspace;
use anyhow::{Context, Result};
use pathbuf::pathbuf;
use which::which;
use xshell::{Shell, cmd};

/// Run the `buf lint` command
pub fn run() -> Result<()> {
	let sh = Shell::new().context("could not init shell")?;
	run_buf_lint(&sh)
}

/// Use existing `xshell::Shell` to run `buf lint`
pub fn run_buf_lint(sh: &Shell) -> Result<()> {
	which("buf").context("could not find 'buf'")?;

	let root = workspace::root()?;
	let config = pathbuf![&root, ".buf.yaml"];
	let target = pathbuf![&root, "hipcheck-common", "proto"];

	cmd!(sh, "buf lint --config {config} {target}").run()?;

	Ok(())
}
