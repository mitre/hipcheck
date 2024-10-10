// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use xshell::{cmd, Shell};

// Git reference to target.
const REF: &str = "fddb21f";

// The Hipcheck repository itself.
const PKG: &str = "pkg:github/mitre/hipcheck";

// The current `hc` binary built from source.
const HC: &str = env!("CARGO_BIN_EXE_hc");

#[test]
fn self_run() -> Result<()> {
	let sh = Shell::new()?;
	cmd!(sh, "{HC} setup").run()?;
	let result = cmd!(sh, "{HC} check -v quiet --ref {REF} {PKG}").read()?;
	println!("{}", result);
	Ok(())
}

#[test]
fn plugin_init() -> Result<()> {
	let sh = Shell::new()?;
	cmd!(sh, "{HC} setup").run()?;
	let result = cmd!(sh, "{HC} plugin").read()?;
	println!("{}", result);
	Ok(())
}
