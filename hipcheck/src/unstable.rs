use crate::{
	cli::{CliConfig, UnstableArgs},
	session::session::Check,
	shell::{ColorChoice, Output},
};
use std::io::Write;
use std::{io, time::Instant};
use tempdir::TempDir;

/// Handle unstable commands.
pub fn main(args: UnstableArgs, config: &CliConfig) -> io::Result<()> {
	// Warn the user that they are using unstable stuff.
	println!("THIS COMMAND IS UNSTABLE. USE AT YOUR OWN RISK.");
	// Get an empty temp dir to force hipcheck to run with no cache.
	let temp_dir = TempDir::new("hipcheck-benchmarking-")?;

	// Destructure and convert check arg.
	let UnstableArgs { benchmark } = args;
	let as_check: Check = benchmark.as_check();

	// Get hipcheck version.
	let raw_version = env!("CARGO_PKG_VERSION", "can't find Hipcheck package version");

	// Get the start instant.
	let start_instant = Instant::now();

	let (shell, report) = crate::run(
		Output::stdout(ColorChoice::Auto),
		Output::stderr(ColorChoice::Auto),
		config.verbosity(),
		as_check,
		config.config().map(ToOwned::to_owned),
		config.data().map(ToOwned::to_owned),
		// Use an empty temp dir for caching -- we do not want previous cache values to mess up benchmarking.
		Some(temp_dir.path().to_owned()),
		config.format(),
		raw_version,
	);

	// Print error if there was one.
	if let Err(e) = report {
		if shell.error(&e, config.format()).is_err() {
			crate::print_error(&e);
			return Err(io::Error::other("internal hipcheck error"));
		}
	}

	// Print the elapsed wall time, in seconds, with microsecond precision.
	// Use a stdout lock to wait to do this.
	let mut stdout = io::stdout().lock();
	writeln!(
		&mut stdout,
		"\nDONE. ELAPSED WALL TIME: {:.6} SECONDS.",
		(Instant::now() - start_instant).as_secs_f64()
	)
	.unwrap();
	// Drop our lock -- free up standard output for someone else.
	drop(stdout);

	// Close the temp dir.
	temp_dir.close()?;
	// Return ok.
	Ok(())
}
