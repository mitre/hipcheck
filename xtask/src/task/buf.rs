use crate::workspace;
use anyhow::{Context, Result};
use pathbuf::pathbuf;
use which::which;
use xshell::{cmd, Shell};

/// Run the `buf lint` command
pub fn run() -> Result<()> {
	let sh = Shell::new().context("could not init shell")?;
	which("buf").context("could not find 'buf'")?;

	let root = workspace::root()?;
	let config = pathbuf![&root, ".buf.yaml"];
	let target = pathbuf![&root, "hipcheck", "proto"];

	cmd!(sh, "buf lint --config {config} {target}").run()?;

	Ok(())
}
