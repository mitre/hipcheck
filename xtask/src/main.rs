// SPDX-License-Identifier: Apache-2.0

mod task;
mod workspace;

use clap::Parser as _;
use clap_verbosity_flag::Verbosity;
use clap_verbosity_flag::WarnLevel;
use std::process::ExitCode;

fn main() -> ExitCode {
	let args = Args::parse();

	env_logger::Builder::new()
		.filter_level(args.verbose.log_level_filter())
		.format_timestamp(None)
		.format_module_path(false)
		.format_target(false)
		.format_indent(Some(8))
		.init();

	let result = match args.command {
		Commands::Check => task::check::run(),
		Commands::Ci => task::ci::run(),
		Commands::Changelog(args) => task::changelog::run(args),
	};

	match result {
		Ok(_) => ExitCode::SUCCESS,
		Err(e) => {
			log::error!("{}", e);
			ExitCode::FAILURE
		}
	}
}

/// Hipcheck development task runner.
#[derive(Debug, clap::Parser)]
#[clap(about, version, long_about = None, propagate_version = true)]
struct Args {
	#[clap(flatten)]
	verbose: Verbosity<WarnLevel>,

	#[clap(subcommand)]
	command: Commands,
}

#[derive(Debug, clap::Subcommand)]
enum Commands {
	/// Run a variety of quality checks.
	Check,
	/// Simulate a CI run locally.
	Ci,
	/// Generate a draft CHANGELOG
	Changelog(ChangelogArgs),
}

#[derive(Debug, clap::Args)]
pub struct ChangelogArgs {
	/// Whether to bump the version, else new commits are "unreleased"
	#[clap(short = 'b', long = "bump")]
	bump: bool,
}

#[cfg(test)]
mod tests {
	use super::Args;
	use clap::CommandFactory;

	#[test]
	fn verify_cli() {
		Args::command().debug_assert()
	}
}
