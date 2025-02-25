// SPDX-License-Identifier: Apache-2.0

mod task;
mod workspace;

use clap::Parser as _;
use clap_verbosity_flag::{InfoLevel, Verbosity};
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
		Commands::Buf => task::buf::run(),
		Commands::Check => task::check::run(),
		Commands::Ci => task::ci::run(),
		Commands::Changelog(args) => task::changelog::run(args),
		Commands::Rfd(args) => task::rfd::run(args),
		Commands::Site(args) => match args.command {
			SiteCommand::Serve(args) => task::site::serve::run(args),
		},
		Commands::Manifest => task::manifest::run(),
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
	verbose: Verbosity<InfoLevel>,

	#[clap(subcommand)]
	command: Commands,
}

#[derive(Debug, clap::Subcommand)]
enum Commands {
	/// Run a variety of quality checks
	Check,
	/// Simulate a CI run locally
	Ci,
	/// Generate a draft CHANGELOG
	Changelog(ChangelogArgs),
	/// Interact with Hipcheck RFDs
	Rfd(RfdArgs),
	/// Work with the Hipcheck website.
	Site(SiteArgs),
	/// Lint the Hipcheck plugin gRPC definition.
	Buf,
	/// Update the plugin download manifests in the local repo based on
	/// output from the GitHub Releases API
	Manifest,
}

#[derive(Debug, clap::Args)]
struct ChangelogArgs {
	/// Whether to bump the version, else new commits are "unreleased"
	#[clap(short = 'b', long = "bump")]
	bump: bool,
}

#[derive(Debug, clap::Args)]
struct RfdArgs {
	#[clap(subcommand)]
	command: RfdCommand,
}

#[derive(Debug, clap::Subcommand)]
enum RfdCommand {
	/// List existing RFDs
	List,
	/// Create a new RFD
	New(NewRfdArgs),
}

#[derive(Debug, clap::Args)]
struct NewRfdArgs {
	/// The ID number to use for the RFD; inferred by default
	#[arg(short = 'n', long = "number")]
	number: Option<u16>,

	/// The title to give the RFD
	title: String,
}

#[derive(Debug, clap::Args)]
struct SiteArgs {
	#[clap(subcommand)]
	command: SiteCommand,
}

#[derive(Debug, clap::Subcommand)]
enum SiteCommand {
	/// Serve the local development site.
	Serve(SiteServeArgs),
}

#[derive(Debug, clap::Args)]
struct SiteServeArgs {
	/// The environment to run the site for.
	#[arg(short = 'e', long = "env")]
	env: Option<SiteEnvironment>,
}

impl SiteServeArgs {
	/// Get the selected environment.
	fn env(&self) -> SiteEnvironment {
		self.env.unwrap_or_default()
	}
}

#[derive(Debug, Default, Clone, Copy, clap::ValueEnum)]
enum SiteEnvironment {
	#[default]
	Dev,
	Prod,
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
