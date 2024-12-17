// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use which::which;
use xshell::Shell;

use super::ci::run_buf_lint;

/// Run the `buf lint` command
pub fn run() -> Result<()> {
	let sh = Shell::new().context("could not init shell")?;
	which("buf").context("could not find 'buf'")?;
	run_buf_lint(&sh)?;
	Ok(())
}
