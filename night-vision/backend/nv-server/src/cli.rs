//! Define the `nv-server` Command Line Interface (CLI).

use crate::env::Env;
use camino::{Utf8Path, Utf8PathBuf};
use clap::{ArgMatches, Args as _, FromArgMatches as _};
use clap_verbosity_flag::{InfoLevel, Verbosity, VerbosityFilter};
use nv_common::config::DEFAULT_CONFIG_FILE;

#[derive(Debug)]
pub struct Cli {
	matches: ArgMatches,
}

#[derive(Debug, PartialEq, Eq)]
pub enum RunMode {
	Normal,
	WriteOpenApi,
}

impl Cli {
	/// Get the args passed on the CLI.
	pub fn args(env: &Env) -> Self {
		// NOTE: In general, prefer to add items to the configuration file rather
		// than adding flags to the CLI. The configuration file can be tracked
		// and managed more easily, and is the standard way to configure the server.
		let command = clap::Command::new(env.bin_name())
			.about("Night Vision backend server")
			.version(env.bin_short_version())
			.long_version(env.bin_long_version())
			.arg(
				clap::Arg::new("config")
					.short('c')
					.long("config")
					.value_name("FILE")
					.value_parser(clap::value_parser!(Utf8PathBuf))
					.default_value(DEFAULT_CONFIG_FILE)
					.help("Path to the configuration file"),
			)
			.arg(
				clap::Arg::new("openapi")
					.long("openapi")
					.action(clap::ArgAction::SetTrue)
					.help("Write the OpenAPI Description and exit without starting the server"),
			);
		let command = Verbosity::<InfoLevel>::augment_args(command);
		let matches = command.get_matches();

		Self { matches }
	}

	/// Get the config path provided by the user.
	pub fn config_path(&self) -> &Utf8Path {
		self.matches
			.get_one::<Utf8PathBuf>("config")
			.expect("config path is required")
	}

	/// Get the verbosity filter level, as configured by
	/// `-v --verbose` and `-q --quiet` flags.
	pub fn verbosity_filter(&self) -> VerbosityFilter {
		let verbosity = Verbosity::<InfoLevel>::from_arg_matches(&self.matches)
			.expect("verbose and quiet args are always present and default to 0");
		verbosity.filter()
	}

	pub fn run_mode(&self) -> RunMode {
		if self.matches.get_flag("openapi") {
			RunMode::WriteOpenApi
		} else {
			RunMode::Normal
		}
	}
}
