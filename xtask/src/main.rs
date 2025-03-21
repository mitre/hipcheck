// SPDX-License-Identifier: Apache-2.0

mod build;
mod string;
mod task;
mod workspace;

use clap::{
	builder::{IntoResettable, PossibleValue, Resettable, Str},
	Parser as _,
};
use clap_verbosity_flag::{InfoLevel, Verbosity};
use std::{fmt::Display, process::ExitCode};

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
		Commands::Build(args) => task::build::run(args),
		Commands::Check(args) => task::check::run(args),
		Commands::Buf => task::buf::run(),
		Commands::Validate => task::validate::run(),
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

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum HelpHeading {
	Args,
}

impl Display for HelpHeading {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			HelpHeading::Args => write!(f, "Arguments"),
		}
	}
}

impl IntoResettable<Str> for HelpHeading {
	fn into_resettable(self) -> clap::builder::Resettable<Str> {
		match self {
			HelpHeading::Args => Resettable::Value(Str::from(self.to_string())),
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
	/// Rebuild crates in the workspace.
	Build(BuildArgs),
	/// Check compilation for crates in the workspace.
	Check(BuildArgs),
	/// Lint the Hipcheck plugin gRPC definition.
	Buf,
	/// Run a variety of quality checks.
	Validate,
	/// Simulate a CI run locally.
	Ci,
	/// Generate a draft CHANGELOG.
	Changelog(ChangelogArgs),
	/// Update the plugin download manifests after a new release.
	Manifest,
	/// Interact with Hipcheck RFDs.
	Rfd(RfdArgs),
	/// Work with the Hipcheck website.
	Site(SiteArgs),
}

#[derive(Debug, clap::Args)]
struct BuildArgs {
	/// The build profile to use.
	#[clap(long = "profile", default_value_t, help_heading = HelpHeading::Args)]
	profile: BuildProfile,

	/// What to build in the workspace.
	#[clap(short = 'p', long = "pkg", default_values_t = default_pkg(), help_heading = HelpHeading::Args)]
	pkg: Vec<BuildPkg>,

	/// Whether or not to add `--timings` to the cargo command
	#[clap(long = "timings", help_heading = HelpHeading::Args)]
	timings: bool,
}

fn default_pkg() -> impl IntoIterator<Item = BuildPkg> {
	vec![BuildPkg::All]
}

#[derive(Debug, Clone, Copy, Default, clap::ValueEnum)]
enum BuildProfile {
	/// Debug mode.
	#[default]
	Debug,
	/// Release mode.
	Release,
	/// Distribution mode.
	///
	/// Used for prebuilt binaries on releases.
	Dist,
}

impl Display for BuildProfile {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", display_for_value_enum(self).get_name())
	}
}

#[derive(Debug, Clone, Copy, Default, clap::ValueEnum)]
enum BuildPkg {
	/// Rebuild the whole workspace.
	#[default]
	All,
	/// Rebuild Hipcheck core.
	Core,
	/// Rebuild the Rust SDK.
	Sdk,
	/// Rebuild all plugins.
	Plugins,
}

impl Display for BuildPkg {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", display_for_value_enum(self).get_name())
	}
}

fn display_for_value_enum<T: clap::ValueEnum>(t: &T) -> PossibleValue {
	t.to_possible_value()
		.unwrap_or_else(|| PossibleValue::new("<skipped>"))
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
