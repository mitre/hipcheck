// SPDX-License-Identifier: Apache-2.0

mod analysis;
mod cli;
mod command_util;
mod config;
mod context;
mod data;
mod error;
mod report;
mod shell;
#[cfg(test)]
mod test_util;
#[cfg(test)]
mod tests;
mod util;
mod version;

use crate::analysis::report_builder::build_pr_report;
use crate::analysis::report_builder::build_report;
use crate::analysis::report_builder::AnyReport;
use crate::analysis::report_builder::Format;
use crate::analysis::report_builder::PrReport;
use crate::analysis::report_builder::Report;
use crate::analysis::score::score_pr_results;
use crate::analysis::score::score_results;
use crate::analysis::session::resolve_cache;
use crate::analysis::session::resolve_config;
use crate::analysis::session::resolve_data;
use crate::analysis::session::Check;
use crate::analysis::session::Session;
use crate::analysis::session::TargetKind;
use crate::context::Context as _;
use crate::error::Error;
use crate::error::Result;
use crate::shell::Output;
use crate::shell::Shell;
use crate::shell::Verbosity;
use crate::util::iter::TryAny;
use crate::util::iter::TryFilter;
use cli::CheckArgs;
use cli::CliConfig;
use cli::FullCommands;
use cli::SchemaArgs;
use cli::SchemaCommand;
use command_util::DependentProgram;
use env_logger::Builder as EnvLoggerBuilder;
use env_logger::Env;
use schemars::schema_for;
use std::env;
use std::ops::Not as _;
use std::path::Path;
use std::path::PathBuf;
use std::process::ExitCode;

fn init_logging() {
	EnvLoggerBuilder::from_env(Env::new().filter("HC_LOG").write_style("HC_LOG_STYLE")).init();
}

/// Entry point for Hipcheck.
fn main() -> ExitCode {
	init_logging();

	let config = CliConfig::load();

	match config.subcommand() {
		Some(FullCommands::Check(args)) => return cmd_check(&args, &config),
		Some(FullCommands::Schema(args)) => cmd_schema(&args),
		Some(FullCommands::Ready) => cmd_ready(&config),
		Some(FullCommands::PrintConfig) => cmd_print_config(config.config()),
		Some(FullCommands::PrintData) => cmd_print_data(config.data()),
		Some(FullCommands::PrintCache) => cmd_print_home(config.cache()),
		None => print_error(&hc_error!("missing subcommand")),
	}

	ExitCode::SUCCESS
}

/// Run the `check` command.
fn cmd_check(args: &CheckArgs, config: &CliConfig) -> ExitCode {
	let check = match &args.command {
		Some(command) => command.as_check(),
		None => {
			print_error(&hc_error!("unknown check type"));
			return ExitCode::FAILURE;
		}
	};

	if check.kind.target_kind().is_checkable().not() {
		print_missing();
	}

	let raw_version = env!("CARGO_PKG_VERSION", "can't find Hipcheck package version");

	let (shell, report) = run(
		Output::stdout(config.color()),
		Output::stderr(config.color()),
		config.verbosity(),
		check,
		config.config().map(ToOwned::to_owned),
		config.data().map(ToOwned::to_owned),
		config.cache().map(ToOwned::to_owned),
		config.format(),
		raw_version,
	);

	match report {
		Ok(AnyReport::Report(report)) => {
			let _ = shell.report(&mut Output::stdout(config.color()), report, config.format());
			ExitCode::SUCCESS
		}
		Ok(AnyReport::PrReport(pr_report)) => {
			let _ = shell.pr_report(
				&mut Output::stdout(config.color()),
				pr_report,
				config.format(),
			);
			ExitCode::SUCCESS
		}
		Err(e) => {
			if shell.error(&e, config.format()).is_err() {
				print_error(&e);
			}
			ExitCode::FAILURE
		}
	}
}

/// Run the `schema` command.
fn cmd_schema(args: &SchemaArgs) {
	match args.command {
		Some(SchemaCommand::Maven) => print_maven_schema(),
		Some(SchemaCommand::Npm) => print_npm_schema(),
		Some(SchemaCommand::Patch) => print_patch_schema(),
		Some(SchemaCommand::Pypi) => print_pypi_schema(),
		Some(SchemaCommand::Repo) => print_report_schema(),
		Some(SchemaCommand::Request) => print_request_schema(),
		None => {
			print_error(&hc_error!("unknown schema type"));
		}
	}
}

fn cmd_ready(config: &CliConfig) {
	let cache_path = config.cache();
	let config_path = config.config();
	let data_path = config.data();

	let mut failed = false;

	// Print Hipcheck version
	let raw_version = env!("CARGO_PKG_VERSION", "can't find Hipcheck package version");

	let version_text = format!(
		"{} {}",
		env!("CARGO_PKG_NAME"),
		version::get_version(raw_version).unwrap()
	);
	println!("{}", version_text);

	// Check that git is installed and that its version is up to date
	// Print the version number either way
	match data::git::get_git_version() {
		Ok(git_version) => match DependentProgram::Git.check_version(&git_version) {
			// No need to check Boolean value, because currentl check_version() only returns Ok(true) or Err()
			Ok(_) => print!("Found installed {}", git_version),
			Err(err) => {
				print_error(&err);
				failed = true;
			}
		},
		Err(err) => {
			print_error(&err);
			failed = true;
		}
	}

	// Check that git is installed and that its version is up to date
	// Print the version number either way
	match data::npm::get_npm_version() {
		Ok(npm_version) => match DependentProgram::Npm.check_version(&npm_version) {
			// No need to check Boolean value, because currently check_version() only returns Ok(true) or Err()
			Ok(_) => print!("Found installed NPM version {}", npm_version),
			Err(err) => {
				print_error(&err);
				failed = true;
			}
		},
		Err(err) => {
			print_error(&err);
			failed = true;
		}
	}

	// Check that the Hipcheck home folder is findable
	match resolve_cache(cache_path) {
		Ok(path_buffer) => println!("Hipcheck home directory: {}", path_buffer.display()),
		Err(err) => {
			failed = true;
			print_error(&err);
		}
	}

	// Check that the Hipcheck config TOML exists in the designated location
	match resolve_config(config_path) {
		Ok(path_buffer) => println!("Hipcheck config file: {}", path_buffer.display()),
		Err(err) => {
			failed = true;
			print_error(&err);
		}
	}

	// Check that Hipcheck data folder is findable
	match resolve_data(data_path) {
		Ok(path_buffer) => println!("Hipcheck data directory: {}", path_buffer.display()),
		Err(err) => {
			failed = true;
			print_error(&err);
		}
	}

	// Check that a GitHub token has been provided as an environment variable
	// This does not check if the token is valid or not
	// The absence of a token does not trigger the failure state for the readiness check, because
	// Hipcheck *can* run without a token, but some analyses will not.
	if std::env::var("HC_GITHUB_TOKEN").is_ok() {
		println!("HC_GITHUB_TOKEN system environment variable found.");
	} else {
		println!("Missing HC_GITHUB_TOKEN system environment variable. Some analyses will not run without this token set.");
	}

	if failed {
		println!("One or more dependencies or configuration settings are missing. Hipcheck is not ready to run.");
		return;
	}

	println!("Hipcheck is ready to run!");
}

/// Print the current home directory for Hipcheck.
///
/// Exits `Ok` if home directory is specified, `Err` otherwise.
fn cmd_print_home(path: Option<&Path>) {
	let cache = resolve_cache(path);

	match cache {
		Ok(path_buffer) => {
			println!("{}", path_buffer.display());
		}
		Err(err) => {
			print_error(&err);
		}
	}
}

/// Print the current config path for Hipcheck.
///
/// Exits `Ok` if config path is specified, `Err` otherwise.
fn cmd_print_config(config_path: Option<&Path>) {
	let config = resolve_config(config_path);
	match config {
		Ok(path_buffer) => {
			println!("{}", path_buffer.display());
		}
		Err(err) => {
			print_error(&err);
		}
	}
}

/// Print the current data folder path for Hipcheck.
///
/// Exits `Ok` if config path is specified, `Err` otherwise.
fn cmd_print_data(data_path: Option<&Path>) {
	let hipcheck_data = resolve_data(data_path);
	match hipcheck_data {
		Ok(path_buffer) => {
			println!("{}", path_buffer.display());
		}
		Err(err) => {
			print_error(&err);
		}
	}
}

/// Print the JSON schema of the report.
fn print_report_schema() {
	let schema = schema_for!(Report);
	let report_text = serde_json::to_string_pretty(&schema).unwrap();
	println!("{}", report_text);
}

/// Print the JSON schema of the pull/merge request.
fn print_request_schema() {
	let schema = schema_for!(PrReport);
	let report_text = serde_json::to_string_pretty(&schema).unwrap();
	println!("{}", report_text);
}

/// Print the JSON schema of the maven package
fn print_maven_schema() {
	print_missing()
}

/// Print the JSON schema of the npm package
fn print_npm_schema() {
	print_missing()
}

/// Print the JSON schema of the patch.
fn print_patch_schema() {
	print_missing()
}

/// Print the JSON schema of the pypi package
fn print_pypi_schema() {
	print_missing()
}

/// Prints a message telling the user that this functionality has not been implemented yet
fn print_missing() {
	println!("This feature is not implemented yet.");
}

/// An `f64` that is never `NaN`.
type F64 = ordered_float::NotNan<f64>;

// Global variables for toml files per issue 157 config updates
const LANGS_FILE: &str = "Langs.toml";
const BINARY_CONFIG_FILE: &str = "Binary.toml";
const TYPO_FILE: &str = "Typos.toml";
const ORGS_FILE: &str = "Orgs.toml";
const HIPCHECK_TOML_FILE: &str = "Hipcheck.toml";

// Constants for exiting with error codes.
/// Indicates the program failed.
const EXIT_FAILURE: i32 = 1;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum CheckKind {
	Repo,
	Request,
	Patch,
	Maven,
	Npm,
	Pypi,
	Spdx,
}

impl CheckKind {
	/// Get the name of the check.
	const fn name(&self) -> &'static str {
		match self {
			CheckKind::Repo => "repo",
			CheckKind::Request => "request",
			CheckKind::Patch => "patch",
			CheckKind::Maven => "maven",
			CheckKind::Npm => "npm",
			CheckKind::Pypi => "pypi",
			CheckKind::Spdx => "spdx",
		}
	}

	/// Get the kind of target implied by the object being checked.
	const fn target_kind(&self) -> TargetKind {
		match self {
			CheckKind::Repo => TargetKind::RepoSource,
			CheckKind::Request => TargetKind::PrUri,
			CheckKind::Patch => TargetKind::PatchUri,
			CheckKind::Maven => TargetKind::PackageVersion,
			CheckKind::Npm => TargetKind::PackageVersion,
			CheckKind::Pypi => TargetKind::PackageVersion,
			CheckKind::Spdx => TargetKind::SpdxDocument,
		}
	}
}

/// Run Hipcheck.
///
/// Parses arguments, sets up shell output, and then runs the main logic.
#[allow(clippy::too_many_arguments)]
fn run(
	output: Output,
	error_output: Output,
	verbosity: Verbosity,
	check: Check,
	config_path: Option<PathBuf>,
	data_path: Option<PathBuf>,
	home_dir: Option<PathBuf>,
	format: Format,
	raw_version: &str,
) -> (Shell, Result<AnyReport>) {
	// Setup wrapper for shell output.
	let shell = Shell::new(output, error_output, verbosity);

	// Run and print / report errors.
	run_with_shell(
		shell,
		check,
		config_path,
		data_path,
		home_dir,
		format,
		raw_version,
	)
}

// This is for testing purposes.
/// Now that we're fully-initialized, run Hipcheck's analyses.
#[allow(clippy::too_many_arguments)]
#[doc(hidden)]
fn run_with_shell(
	shell: Shell,
	check: Check,
	config_path: Option<PathBuf>,
	data_path: Option<PathBuf>,
	home_dir: Option<PathBuf>,
	format: Format,
	raw_version: &str,
) -> (Shell, Result<AnyReport>) {
	// Initialize the session.
	let session = match Session::new(
		shell,
		&check,
		&check.target,
		config_path,
		data_path,
		home_dir,
		format,
		raw_version,
	) {
		Ok(session) => session,
		Err((shell, err)) => return (shell, Err(err)),
	};

	match check.kind.target_kind() {
		TargetKind::RepoSource | TargetKind::SpdxDocument => {
			// Run analyses against a repo and score the results (score calls analyses that call metrics).
			let mut phase = match session.shell.phase("analyzing and scoring results") {
				Ok(phase) => phase,
				Err(err) => return (session.end(), Err(err)),
			};

			let scoring = match score_results(&mut phase, &session) {
				Ok(scoring) => scoring,
				_ => {
					return (
						session.end(),
						Err(Error::msg("Trouble scoring and analyzing results")),
					)
				}
			};

			match phase.finish() {
				Ok(()) => {}
				Err(err) => return (session.end(), Err(err)),
			};

			// Build the final report.
			let report =
				match build_report(&session, &scoring).context("failed to build final report") {
					Ok(report) => report,
					Err(err) => return (session.end(), Err(err)),
				};

			(session.end(), Ok(AnyReport::Report(report)))
		}
		TargetKind::PackageVersion => {
			// Run analyses against a repo and score the results (score calls analyses that call metrics).
			let mut phase = match session.shell.phase("analyzing and scoring results") {
				Ok(phase) => phase,
				Err(err) => return (session.end(), Err(err)),
			};

			let scoring = match score_results(&mut phase, &session) {
				Ok(scoring) => scoring,
				_ => {
					return (
						session.end(),
						Err(Error::msg("Trouble scoring and analyzing results")),
					)
				}
			};

			match phase.finish() {
				Ok(()) => {}
				Err(err) => return (session.end(), Err(err)),
			};

			// Build the final report.
			let report =
				match build_report(&session, &scoring).context("failed to build final report") {
					Ok(report) => report,
					Err(err) => return (session.end(), Err(err)),
				};

			(session.end(), Ok(AnyReport::Report(report)))
		}
		TargetKind::PrUri => {
			// Run analyses against a pull request and score the results (score calls analyses that call metrics).
			let mut phase = match session.shell.phase("scoring and analyzing results") {
				Ok(phase) => phase,
				Err(err) => return (session.end(), Err(err)),
			};

			let score = match score_pr_results(&mut phase, &session) {
				Ok(score) => score,
				_ => {
					return (
						session.end(),
						Err(Error::msg("Trouble scoring and analyzing results")),
					)
				}
			};

			match phase.finish() {
				Ok(()) => {}
				Err(err) => return (session.end(), Err(err)),
			};

			// Build the final report.
			let pr_report =
				match build_pr_report(&session, &score).context("failed to build final report") {
					Ok(pr_report) => pr_report,
					Err(err) => return (session.end(), Err(err)),
				};

			(session.end(), Ok(AnyReport::PrReport(pr_report)))
		}
		_ => (
			session.end(),
			Err(Error::msg(
				"Hipcheck attempted to analyze an unsupported type",
			)),
		),
	}
}

/// Print errors which occur before the `Shell` type can be setup.
fn print_error(err: &Error) {
	let mut chain = err.chain();

	// PANIC: First error is guaranteed to be present.
	eprintln!("error: {}", chain.next().unwrap());

	for err in chain {
		eprintln!("       {}", err);
	}
}
