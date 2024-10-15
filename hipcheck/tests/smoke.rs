// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use pathbuf::pathbuf;
use std::path::PathBuf;
use xshell::{cmd, Shell};

// This test just wraps a shell script, so that the running of this shell
// script can be integrated with Rust's testing infrastructure.
//
// It's done this way because Rust's integration tests feature will only
// ensure compilation of any binary targets associated with the current package,
// not with the broader workspace. In our case, to do the integration test we
// need to make sure we've compiled `hc` AND all the individual plugins under
// the `plugins/` directory. So we delegate all of that to a shell script
// that sits under a `tests/` directory in the top-level of the repository,
// and then just loop it into Rust's test system here.
#[test]
fn integration_script() -> Result<()> {
	let sh = Shell::new()?;
	let root = workspace_root();
	let script = pathbuf![&root, "tests", "integration.sh"];
	cmd!(sh, "{script}").run()?;
	Ok(())
}

fn workspace_root() -> PathBuf {
	let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
	root.parent()
		.expect("manifest dir has parent directory")
		.to_owned()
}
