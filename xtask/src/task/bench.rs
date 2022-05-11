// SPDX-License-Identifier: Apache-2.0

//! Times the running of a full build using command cargo build --release

use crate::exit::EXIT_SUCCESS;
use crate::task::ci::Printer;
use duct::cmd;
use hc_error::{hc_error, Error, Result};
use std::io;
use std::mem::drop;
use std::process::exit;
use std::process::Command;
use std::time::Instant;

/// cargo xtask bench build: Build Hipcheck from-scratch and measure how long it takes.
pub fn build() -> Result<()> {
	//Per ALB, for a fresh build, you'd do cargo clean first, and then time the cargo build --release
	let mut printer = Printer::new();
	printer.header("Running `cargo clean`")?;
	cmd!("cargo", "clean")
		.run()
		.map(drop)
		.map_err(reason("call to `cargo clean` failed"))?;

	//using Command::new for the full cargo build --release because cmd kept failing with signal 9
	printer.header("Getting time benchmark for `cargo build --release`")?;
	let start = Instant::now();
	let output = Command::new("cargo")
		.arg("build")
		.arg("--release")
		.output()
		.expect("Failed to execute `cargo build --release` command");

	println!("status: {}", output.status);
	println!("output:");
	println!("{}", String::from_utf8_lossy(&output.stdout));
	println!("{}", String::from_utf8_lossy(&output.stderr));
	let duration = start.elapsed();
	println!(
		"Full `cargo build --release` duration was: {} seconds or {} minutes",
		duration.as_secs_f64(),
		(duration.as_secs_f64() / 60.0)
	);
	exit(EXIT_SUCCESS);
}

/// Replace an existing error with a new message.
fn reason(msg: &'static str) -> impl FnOnce(io::Error) -> Error {
	move |_| hc_error!("{}", msg)
}
