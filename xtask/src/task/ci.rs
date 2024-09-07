// SPDX-License-Identifier: Apache-2.0

//! A task to simulate a CI run locally.

use anyhow::{anyhow, Error, Result};
use std::{mem::drop, ops::Not as _};
use xshell::{cmd, Shell};

/// Helper type for tasks.
type Task = (&'static str, fn(&Shell) -> Result<()>);

macro_rules! task {
	($fn:ident) => {
		(stringify!($fn), $fn)
	};
}

/// Execute the CI task.
pub fn run() -> Result<()> {
	let sh = Shell::new()?;

	let tasks: &[Task] = &[
		task!(check_using_stable),
		task!(check_stable_is_current),
		task!(check_target_matches_ci),
		task!(print_versions),
		task!(run_fmt),
		task!(run_check),
		task!(run_build),
		task!(run_test),
		task!(run_clippy),
		task!(run_xtask_check),
		task!(done),
	];

	for (name, task) in tasks {
		if let Err(e) = task(&sh) {
			log::error!("task: {}, error: {}", name, e);
		}
	}

	Ok(())
}

/// Check if the active toolchain is stable.
fn check_using_stable(sh: &Shell) -> Result<()> {
	// Get the active toolchain.
	let active_toolchain = cmd!(sh, "rustup show")
		.read()
		.map_err(reason("call to rustup failed. Make sure rust is installed and path to home-dir-here/.cargo/bin is on your path."))?;

	// Check if stable is the default.
	let stable_is_default = active_toolchain.lines().any(|l| {
		(l.contains("default") || l.contains("environment override")) && l.contains("stable")
	});

	// If it isn't, issue an error.
	if stable_is_default.not() {
		return Err(anyhow!(
			"not using stable toolchain. Run 'rustup default stable'"
		));
	}

	Ok(())
}

/// Check if the stable toolchain is up to date.
fn check_stable_is_current(sh: &Shell) -> Result<()> {
	// Check the versions of the toolchains installed by rustup.
	let results = cmd!(sh, "rustup check")
		.read()
		.map_err(reason("call to rustup failed. Make sure rust is installed and path to home-dir-here/.cargo/bin is on your path."))?;

	// Check if stable is considered up to date.
	let stable_is_current = results
		.lines()
		.any(|l| l.contains("stable") && l.contains("Up to date"));

	// If it isn't, issue an error.
	if stable_is_current.not() {
		return Err(anyhow!(
			"stable toolchain is out of date. Run 'rustup update' to make current."
		));
	}

	Ok(())
}

/// Check if the target toolchain matches the CI toolchain.
fn check_target_matches_ci(sh: &Shell) -> Result<()> {
	// Get the toolchain info from rustup.
	let results = cmd!(sh, "rustup show")
		.read()
		.map_err(reason("call to rustup failed. Make sure rust is installed and path to home-dir-here/.cargo/bin is on your path."))?;

	// Extract out just the toolchain string.
	let toolchain = results
		.lines()
		.find(|l| l.contains("Default host: "))
		.ok_or_else(|| anyhow!("missing default host from rustup"))?
		.replace("Default host: ", "");

	// The toolchain used in CI.
	let ci_toolchain = "x86_64-unknown-linux-gnu";

	// Warn if they're different.
	if toolchain != ci_toolchain {
		log::error!(
			"CI and host toolchains don't match! CI toolchain is {}, current host is {}",
			ci_toolchain,
			toolchain
		);
	}

	Ok(())
}

/// Print versions of the tools we use.
fn print_versions(sh: &Shell) -> Result<()> {
	// Print versions of tools.
	print_rustc_version(sh)?;
	print_cargo_version(sh)?;
	print_fmt_version(sh)?;
	print_clippy_version(sh)?;
	print_xtask_version(sh)?;

	Ok(())
}

/// Print the version of `rustc`.
fn print_rustc_version(sh: &Shell) -> Result<()> {
	cmd!(sh, "rustc --version")
		.run()
		.map(drop)
		.map_err(reason("call to rustc failed. Make sure rust is installed and path to home-dir-here/.cargo/bin is on your path."))
}

/// Print the version of `cargo`.
fn print_cargo_version(sh: &Shell) -> Result<()> {
	cmd!(sh, "cargo --version")
		.run()
		.map(drop)
		.map_err(reason("call to cargo failed. Make sure rust is installed and path to home-dir-here/.cargo/bin is on your path."))
}

/// Print the version of `cargo fmt`.
fn print_fmt_version(sh: &Shell) -> Result<()> {
	cmd!(sh, "cargo fmt --version")
		.run()
		.map(drop)
		.map_err(reason("call to cargo fmt failed. Make sure rust is installed and path to home-dir-here/.cargo/bin is on your path."))
}

/// Print the version of `cargo clippy`.
fn print_clippy_version(sh: &Shell) -> Result<()> {
	cmd!(sh, "cargo clippy --version")
		.run()
		.map(drop)
		.map_err(reason("call to cargo clippy failed. Make sure rust is installed and path to home-dir-here/.cargo/bin is on your path."))
}

/// Print the version of `cargo xtask`.
fn print_xtask_version(sh: &Shell) -> Result<()> {
	cmd!(sh,
		"cargo run --package xtask --bin xtask --quiet -- --version"
	)
	.run()
	.map(drop)
	.map_err(reason("call to cargo xtask failed. Make sure rust is installed and path to home-dir-here/.cargo/bin is on your path."))
}

/// Run `cargo fmt`.
fn run_fmt(sh: &Shell) -> Result<()> {
	cmd!(sh, "cargo fmt --all -- --color=always --check")
		.run()
		.map(drop)
		.map_err(reason(
			"call to cargo fmt failed. To automatically fix cargo fmt issues most likely due to white space or tab issues, run
		cargo fmt --all",
		))
}

/// Run `cargo check`.
fn run_check(sh: &Shell) -> Result<()> {
	cmd!(sh, "cargo check --workspace --benches --tests")
		.run()
		.map(drop)
		.map_err(reason("call to cargo failed"))
}

/// Run `cargo build`.
fn run_build(sh: &Shell) -> Result<()> {
	cmd!(sh, "cargo build --bins --benches")
		.run()
		.map(drop)
		.map_err(reason("call to cargo failed"))
}

/// Run `cargo test`.
fn run_test(sh: &Shell) -> Result<()> {
	// Opportunistically use 'cargo-nextest' if present.
	if which::which("cargo-nextest").is_ok() {
		cmd!(sh, "cargo nextest run --workspace")
			.run()
			.map(drop)
			.map_err(reason("call to cargo-nextest failed"))
	} else {
		cmd!(sh, "cargo test --workspace")
			.run()
			.map(drop)
			.map_err(reason("call to cargo failed"))
	}
}

/// Run `cargo clippy`.
fn run_clippy(sh: &Shell) -> Result<()> {
	cmd!(sh, "cargo clippy --workspace --all-targets -- -D warnings")
		.run()
		.map(drop)
		.map_err(reason("call to cargo clippy failed"))
}

/// Run `cargo xtask check`.
fn run_xtask_check(sh: &Shell) -> Result<()> {
	cmd!(sh, "cargo run --package xtask --bin xtask --quiet -- check")
		.run()
		.map(drop)
		.map_err(reason("call to cargo xtask failed"))
}

/// Tell the user we're done.
fn done(_sh: &Shell) -> Result<()> {
	log::info!(
		"task: {}, message: All checks passed! You can expect to pass CI now.",
		"Done"
	);
	Ok(())
}

/// Replace an existing error with a new message.
fn reason(msg: &'static str) -> impl FnOnce(xshell::Error) -> Error {
	move |_| anyhow!("{}", msg)
}
