// SPDX-License-Identifier: Apache-2.0

//! Installs binaries from the workspace.

use crate::exit::EXIT_SUCCESS;
use anyhow::anyhow as hc_error;
use anyhow::Error;
use anyhow::Result;
use duct::cmd;
use std::io;
use std::mem::drop;
use std::process::exit;

/// Install the requested target.
pub fn run() -> Result<()> {
	cmd!("cargo", "install", "--path", "hipcheck")
		.run()
		.map(drop)
		.map_err(reason("call to cargo failed"))?;

	exit(EXIT_SUCCESS);
}

/// Replace an existing error with a new message.
fn reason(msg: &'static str) -> impl FnOnce(io::Error) -> Error {
	move |_| hc_error!("{}", msg)
}
