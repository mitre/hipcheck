// SPDX-License-Identifier: Apache-2.0

//! A task to simulate a CI run locally.

use crate::exit::EXIT_SUCCESS;
use duct::cmd;
use hc_common::{
	error::{Error, Result},
	hc_error,
};
use hc_pathbuf::pathbuf;
use std::io;
use std::mem::drop;
use std::path::PathBuf;
use std::process::exit;

/// Execute the CI task.
pub fn run(should_open: OpenDoc) -> Result<()> {
	run_cargo_doc(should_open)?;
	exit(EXIT_SUCCESS);
}

/// Print the version of `rustc`.
fn run_cargo_doc(should_open: OpenDoc) -> Result<()> {
	cmd!(
		"cargo",
		"doc",
		"--no-deps",
		"--document-private-items",
		"--workspace",
	)
	.run()
	.map(drop)
	.map_err(reason("call to cargo doc failed"))?;

	if should_open.yes() {
		let path = {
			// Get the path to the xtask crate.
			let mut out_dir = option_env!("CARGO_MANIFEST_DIR")
				.map(PathBuf::from)
				.ok_or_else(|| hc_error!("can't find workspace root"))?;

			// Go up one level to reach the workspace root.
			out_dir.pop();

			// Add the rest of the path to the Hipcheck index file.
			pathbuf![&out_dir, ".target", "doc", "hipcheck", "index.html"]
		};

		open::that(path)?;
	}

	Ok(())
}

#[derive(Debug)]
pub enum OpenDoc {
	Yes,
	No,
}

impl OpenDoc {
	fn yes(&self) -> bool {
		match self {
			OpenDoc::Yes => true,
			OpenDoc::No => false,
		}
	}
}

impl From<bool> for OpenDoc {
	fn from(cond: bool) -> Self {
		if cond {
			OpenDoc::Yes
		} else {
			OpenDoc::No
		}
	}
}

/// Replace an existing error with a new message.
fn reason(msg: &'static str) -> impl FnOnce(io::Error) -> Error {
	move |_| hc_error!("{}", msg)
}
