// SPDX-License-Identifier: Apache-2.0

mod task;
mod workspace;

use clap::Parser;
use std::process::ExitCode;

fn main() -> ExitCode {
	let args = Args::parse();

	env_logger::Builder::new()
		.filter_level(args.verbose.log_level_filter())
		.init();

	match args.command {
		Commands::Validate => task::validate::run(),
		Commands::Ci => task::ci::run(),
	}
}

/// Hipcheck development task runner.
#[derive(Debug, clap::Parser)]
#[clap(about, version)]
struct Args {
	#[clap(flatten)]
	verbose: clap_verbosity_flag::Verbosity,

	#[clap(subcommand)]
	command: Commands,
}

#[derive(Debug, clap::Subcommand)]
enum Commands {
	/// Run a variety of quality checks.
	Validate,
	/// Simulate a CI run locally.
	Ci,
}
