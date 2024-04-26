// SPDX-License-Identifier: Apache-2.0

//! A task to simulate a CI run locally.

use crate::exit::EXIT_SUCCESS;
use duct::cmd;
use hc_common::{hc_error, Error, Result};
use std::io::{self, Stderr, Stdout, Write as _};
use std::mem::drop;
use std::ops::Not as _;
use std::process::exit;

/// Execute the CI task.
pub fn run() -> Result<()> {
	let mut printer = Printer::new();

	let actions = &[
		check_using_stable,
		check_stable_is_current,
		check_target_matches_ci,
		print_versions,
		run_fmt,
		run_check,
		run_build,
		run_test,
		run_clippy,
		run_xtask_validate,
		done,
	];

	for action in actions {
		action(&mut printer)?;
	}

	exit(EXIT_SUCCESS);
}

/// Convenience macro to writeln! with a flush.
macro_rules! writelnf {
    ($dst:expr $(,)?) => {{
        write!($dst, "\n")?;
		$dst.flush()
	}};
    ($dst:expr, $($arg:tt)*) => {{
        $dst.write_fmt(format_args!($($arg)*))?;
		write!($dst, "\n")?;
		$dst.flush()
	}};
}

/// Check if the active toolchain is stable.
fn check_using_stable(_printer: &mut Printer) -> Result<()> {
	// Get the active toolchain.
	let active_toolchain = cmd!("rustup", "show")
		.read()
		.map_err(reason("call to rustup failed. Make sure rust is installed and path to home-dir-here/.cargo/bin is on your path."))?;

	// Check if stable is the default.
	let stable_is_default = active_toolchain.lines().any(|l| {
		(l.contains("default") || l.contains("environment override")) && l.contains("stable")
	});

	// If it isn't, issue an error.
	if stable_is_default.not() {
		return Err(hc_error!(
			"not using stable toolchain. Run 'rustup default stable'"
		));
	}

	Ok(())
}

/// Check if the stable toolchain is up to date.
fn check_stable_is_current(_printer: &mut Printer) -> Result<()> {
	// Check the versions of the toolchains installed by rustup.
	let results = cmd!("rustup", "check")
		.read()
		.map_err(reason("call to rustup failed. Make sure rust is installed and path to home-dir-here/.cargo/bin is on your path."))?;

	// Check if stable is considered up to date.
	let stable_is_current = results
		.lines()
		.any(|l| l.contains("stable") && l.contains("Up to date"));

	// If it isn't, issue an error.
	if stable_is_current.not() {
		return Err(hc_error!(
			"stable toolchain is out of date. Run 'rustup update' to make current."
		));
	}

	Ok(())
}

/// Check if the target toolchain matches the CI toolchain.
fn check_target_matches_ci(printer: &mut Printer) -> Result<()> {
	// Get the toolchain info from rustup.
	let results = cmd!("rustup", "show")
		.read()
		.map_err(reason("call to rustup failed. Make sure rust is installed and path to home-dir-here/.cargo/bin is on your path."))?;

	// Extract out just the toolchain string.
	let toolchain = results
		.lines()
		.find(|l| l.contains("Default host: "))
		.ok_or_else(|| hc_error!("missing default host from rustup"))?
		.replace("Default host: ", "");

	// The toolchain used in CI.
	let ci_toolchain = "x86_64-unknown-linux-gnu";

	// Warn if they're different.
	if toolchain != ci_toolchain {
		writelnf!(
			printer.err,
			"warn: CI toolchain is {}, current host is {}.",
			ci_toolchain,
			toolchain
		)?;
		writelnf!(
			printer.err,
			"      Differences may arise because of platform-specific code. Consider running 'rustup default stable'."
		)?;
	}

	Ok(())
}

/// Print versions of the tools we use.
fn print_versions(printer: &mut Printer) -> Result<()> {
	printer.header("Versions")?;

	// Print versions of tools.
	print_rustc_version()?;
	print_cargo_version()?;
	print_fmt_version()?;
	print_clippy_version()?;
	print_xtask_version()?;

	Ok(())
}

/// Print the version of `rustc`.
fn print_rustc_version() -> Result<()> {
	cmd!("rustc", "--version")
		.run()
		.map(drop)
		.map_err(reason("call to rustc failed. Make sure rust is installed and path to home-dir-here/.cargo/bin is on your path."))
}

/// Print the version of `cargo`.
fn print_cargo_version() -> Result<()> {
	cmd!("cargo", "--version")
		.run()
		.map(drop)
		.map_err(reason("call to cargo failed. Make sure rust is installed and path to home-dir-here/.cargo/bin is on your path."))
}

/// Print the version of `cargo fmt`.
fn print_fmt_version() -> Result<()> {
	cmd!("cargo", "fmt", "--version")
		.run()
		.map(drop)
		.map_err(reason("call to cargo fmt failed. Make sure rust is installed and path to home-dir-here/.cargo/bin is on your path."))
}

/// Print the version of `cargo clippy`.
fn print_clippy_version() -> Result<()> {
	cmd!("cargo", "clippy", "--version")
		.run()
		.map(drop)
		.map_err(reason("call to cargo clippy failed. Make sure rust is installed and path to home-dir-here/.cargo/bin is on your path."))
}

/// Print the version of `cargo xtask`.
fn print_xtask_version() -> Result<()> {
	cmd!(
		"cargo",
		"run",
		"--package",
		"xtask",
		"--bin",
		"xtask",
		"--quiet",
		"--",
		"--version"
	)
	.run()
	.map(drop)
	.map_err(reason("call to cargo xtask failed. Make sure rust is installed and path to home-dir-here/.cargo/bin is on your path."))
}

/// Run `cargo fmt`.
fn run_fmt(printer: &mut Printer) -> Result<()> {
	printer.header("cargo fmt")?;

	cmd!("cargo", "fmt", "--all", "--", "--color=always", "--check")
		.run()
		.map(drop)
		.map_err(reason(
			"call to cargo fmt failed. To automatically fix cargo fmt issues most likely due to white space or tab issues, run 
		cargo fmt --all",
		))
}

/// Run `cargo check`.
fn run_check(printer: &mut Printer) -> Result<()> {
	printer.header("cargo check")?;

	cmd!("cargo", "check", "--workspace", "--benches", "--tests")
		.run()
		.map(drop)
		.map_err(reason("call to cargo failed"))
}

/// Run `cargo build`.
fn run_build(printer: &mut Printer) -> Result<()> {
	printer.header("cargo build")?;

	cmd!("cargo", "build", "--bins", "--benches")
		.run()
		.map(drop)
		.map_err(reason("call to cargo failed"))
}

/// Run `cargo test`.
fn run_test(printer: &mut Printer) -> Result<()> {
	printer.header("cargo test")?;

	cmd!("cargo", "test", "--workspace")
		.run()
		.map(drop)
		.map_err(reason("call to cargo failed"))
}

/// Run `cargo clippy`.
fn run_clippy(printer: &mut Printer) -> Result<()> {
	printer.header("cargo clippy")?;

	cmd!(
		"cargo",
		"clippy",
		"--workspace",
		"--all-targets",
		"--",
		"-D",
		"warnings"
	)
	.run()
	.map(drop)
	.map_err(reason("call to cargo clippy failed"))
}

/// Run `cargo xtask validate`.
fn run_xtask_validate(printer: &mut Printer) -> Result<()> {
	printer.header("cargo xtask validate")?;

	cmd!(
		"cargo",
		"run",
		"--package",
		"xtask",
		"--bin",
		"xtask",
		"--quiet",
		"--",
		"validate"
	)
	.run()
	.map(drop)
	.map_err(reason("call to cargo xtask failed"))
}

/// Tell the user we're done.
fn done(printer: &mut Printer) -> Result<()> {
	printer.header("Done")?;
	println!("All checks passed! You can expect to pass CI now.");
	Ok(())
}

/// Holds access to stdout and stderr.
pub struct Printer {
	/// Handle for stdout.
	out: Stdout,
	/// Handle for stderr.
	err: Stderr,
}

impl Printer {
	/// Construct a new `Printer`.
	pub fn new() -> Printer {
		Self {
			out: io::stdout(),
			err: io::stderr(),
		}
	}

	/// Print the header for a section.
	pub fn header(&mut self, msg: &str) -> Result<()> {
		let mut out = self.out.lock();
		writelnf!(out)?;
		writelnf!(out, "{:=<78}", "=")?;
		writelnf!(out, "{}", msg)?;
		writelnf!(out, "{:-<78}", "-")?;
		writelnf!(out)?;
		Ok(())
	}
}

/// Replace an existing error with a new message.
fn reason(msg: &'static str) -> impl FnOnce(io::Error) -> Error {
	move |_| hc_error!("{}", msg)
}
