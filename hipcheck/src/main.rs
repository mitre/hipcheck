// SPDX-License-Identifier: Apache-2.0

#[allow(unused)]
mod analysis;
mod cli;
mod command_util;
mod config;
mod context;
mod data;
mod error;
mod http;
mod metric;
mod report;
mod session;
mod shell;
mod source;
mod target;
#[cfg(test)]
mod test_util;
mod util;
mod version;

#[cfg(feature = "benchmarking")]
mod benchmarking;

use crate::analysis::report_builder::build_pr_report;
use crate::analysis::report_builder::build_report;
use crate::analysis::report_builder::AnyReport;
use crate::analysis::report_builder::Format;
use crate::analysis::report_builder::PrReport;
use crate::analysis::report_builder::Report;
use crate::analysis::score::score_pr_results;
use crate::analysis::score::score_results;
use crate::context::Context as _;
use crate::error::Error;
use crate::error::Result;
use crate::session::session::Check;
use crate::session::session::Session;
use crate::session::session::TargetKind;
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
use core::fmt;
use env_logger::Builder as EnvLoggerBuilder;
use env_logger::Env;
use pathbuf::pathbuf;
use schemars::schema_for;
use std::env;
use std::fmt::Display;
use std::fmt::Formatter;
use std::ops::Not as _;
use std::path::Path;
use std::path::PathBuf;
use std::process::ExitCode;
use std::result::Result as StdResult;
use util::fs::create_dir_all;

fn init_logging() {
	EnvLoggerBuilder::from_env(Env::new().filter("HC_LOG").write_style("HC_LOG_STYLE")).init();
}

/// Entry point for Hipcheck.
fn main() -> ExitCode {
	init_logging();

	if cfg!(feature = "print-timings") {
		println!("[TIMINGS]: Timing information will be printed.");
	}

	// Start tracking the timing for `main` after logging is initiated.
	#[cfg(feature = "print-timings")]
	let _0 = benchmarking::print_scope_time!("main");

	let config = CliConfig::load();

	match config.subcommand() {
		Some(FullCommands::Check(args)) => return cmd_check(&args, &config),
		Some(FullCommands::Schema(args)) => cmd_schema(&args),
		Some(FullCommands::Ready) => cmd_ready(&config),
		Some(FullCommands::PrintConfig) => cmd_print_config(config.config()),
		Some(FullCommands::PrintData) => cmd_print_data(config.data()),
		Some(FullCommands::PrintCache) => cmd_print_home(config.cache()),
		None => print_error(&hc_error!("missing subcommand")),
	};

	// If we didn't early return, return success.
	ExitCode::SUCCESS
}

/// Run the `check` command.
fn cmd_check(args: &CheckArgs, config: &CliConfig) -> ExitCode {
	let check = match args.command() {
		Ok(chk) => chk.as_check(),
		Err(e) => {
			print_error(&e);
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
		SchemaCommand::Maven => print_maven_schema(),
		SchemaCommand::Npm => print_npm_schema(),
		SchemaCommand::Patch => print_patch_schema(),
		SchemaCommand::Pypi => print_pypi_schema(),
		SchemaCommand::Repo => print_report_schema(),
		SchemaCommand::Request => print_request_schema(),
	}
}

#[derive(Debug)]
struct ReadyChecks {
	hipcheck_version_check: StdResult<String, VersionCheckError>,
	git_version_check: StdResult<String, VersionCheckError>,
	npm_version_check: StdResult<String, VersionCheckError>,
	config_path_check: StdResult<PathBuf, PathCheckError>,
	data_path_check: StdResult<PathBuf, PathCheckError>,
	cache_path_check: StdResult<PathBuf, PathCheckError>,
	github_token_check: StdResult<(), EnvVarCheckError>,
}

impl ReadyChecks {
	/// Check if Hipcheck is ready to run.
	///
	/// We don't check `github_token_check`, because it's allowed to fail.
	fn is_ready(&self) -> bool {
		self.hipcheck_version_check.is_ok()
			&& self.git_version_check.is_ok()
			&& self.npm_version_check.is_ok()
			&& self.config_path_check.is_ok()
			&& self.data_path_check.is_ok()
			&& self.cache_path_check.is_ok()
	}
}

#[derive(Debug)]
struct VersionCheckError {
	cmd_name: &'static str,
	kind: VersionCheckErrorKind,
}

impl Display for VersionCheckError {
	fn fmt(&self, f: &mut Formatter) -> fmt::Result {
		match &self.kind {
			VersionCheckErrorKind::CmdNotFound => {
				write!(f, "command '{}' not found", self.cmd_name)
			}
			VersionCheckErrorKind::VersionTooOld { expected, found } => write!(
				f,
				"command '{}' version is too old; found {}, must be at least {}",
				self.cmd_name, found, expected
			),
		}
	}
}

#[derive(Debug)]
enum VersionCheckErrorKind {
	CmdNotFound,
	VersionTooOld { expected: String, found: String },
}

#[derive(Debug)]
enum PathCheckError {
	PathNotFound,
}

impl Display for PathCheckError {
	fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
		match self {
			PathCheckError::PathNotFound => write!(f, "path not found"),
		}
	}
}

#[derive(Debug)]
struct EnvVarCheckError {
	name: &'static str,
	kind: EnvVarCheckErrorKind,
}

impl Display for EnvVarCheckError {
	fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
		match &self.kind {
			EnvVarCheckErrorKind::VarNotFound => {
				write!(f, "environment variable '{}' was not found", self.name)
			}
		}
	}
}

#[derive(Debug)]
enum EnvVarCheckErrorKind {
	VarNotFound,
}

fn check_hipcheck_version() -> StdResult<String, VersionCheckError> {
	let pkg_name = env!("CARGO_PKG_NAME", "can't find Hipcheck package name");

	let version = env!("CARGO_PKG_VERSION", "can't find Hipcheck package version");
	let version = version::get_version(version).map_err(|_| VersionCheckError {
		cmd_name: "hc",
		kind: VersionCheckErrorKind::CmdNotFound,
	})?;

	Ok(format!("{} {}", pkg_name, version))
}

fn check_git_version() -> StdResult<String, VersionCheckError> {
	let version = data::git::get_git_version().map_err(|_| VersionCheckError {
		cmd_name: "git",
		kind: VersionCheckErrorKind::CmdNotFound,
	})?;

	DependentProgram::Git
		.check_version(&version)
		.map(|_| version.trim().to_owned())
		.map_err(|_| VersionCheckError {
			cmd_name: "git",
			kind: VersionCheckErrorKind::VersionTooOld {
				expected: DependentProgram::Git.min_version().unwrap().to_string(),
				found: version,
			},
		})
}

fn check_npm_version() -> StdResult<String, VersionCheckError> {
	let version = data::npm::get_npm_version()
		.map(|version| version.trim().to_owned())
		.map_err(|_| VersionCheckError {
			cmd_name: "npm",
			kind: VersionCheckErrorKind::CmdNotFound,
		})?;

	DependentProgram::Npm
		.check_version(&version)
		.map(|_| version.clone())
		.map_err(|_| VersionCheckError {
			cmd_name: "npm",
			kind: VersionCheckErrorKind::VersionTooOld {
				expected: DependentProgram::Npm.min_version().unwrap().to_string(),
				found: version,
			},
		})
}

fn check_config_path(config: &CliConfig) -> StdResult<PathBuf, PathCheckError> {
	let path = config.config().ok_or(PathCheckError::PathNotFound)?;

	let path = pathbuf![path, HIPCHECK_TOML_FILE];

	if path.exists().not() {
		return Err(PathCheckError::PathNotFound);
	}

	Ok(path)
}

fn check_cache_path(config: &CliConfig) -> StdResult<PathBuf, PathCheckError> {
	let path = config.cache().ok_or(PathCheckError::PathNotFound)?;

	// Try to create the cache directory if it doesn't exist.
	if path.exists().not() {
		create_dir_all(path).map_err(|_| PathCheckError::PathNotFound)?;
	}

	Ok(path.to_owned())
}

fn check_data_path(config: &CliConfig) -> StdResult<PathBuf, PathCheckError> {
	let path = config.data().ok_or(PathCheckError::PathNotFound)?;

	if path.exists().not() {
		return Err(PathCheckError::PathNotFound);
	}

	Ok(path.to_owned())
}

/// Check that a GitHub token has been provided as an environment variable
/// This does not check if the token is valid or not
/// The absence of a token does not trigger the failure state for the readiness check, because
/// Hipcheck *can* run without a token, but some analyses will not.
fn check_github_token() -> StdResult<(), EnvVarCheckError> {
	let name = "HC_GITHUB_TOKEN";

	std::env::var(name)
		.map(|_| ())
		.map_err(|_| EnvVarCheckError {
			name,
			kind: EnvVarCheckErrorKind::VarNotFound,
		})
}

fn cmd_ready(config: &CliConfig) {
	let ready = ReadyChecks {
		hipcheck_version_check: check_hipcheck_version(),
		git_version_check: check_git_version(),
		npm_version_check: check_npm_version(),
		config_path_check: check_config_path(config),
		data_path_check: check_data_path(config),
		cache_path_check: check_cache_path(config),
		github_token_check: check_github_token(),
	};

	match &ready.hipcheck_version_check {
		Ok(version) => println!("{:<17} {}", "Hipcheck Version:", version),
		Err(e) => println!("{:<17} {}", "Hipcheck Version:", e),
	}

	match &ready.git_version_check {
		Ok(version) => println!("{:<17} {}", "Git Version:", version),
		Err(e) => println!("{:<17} {}", "Git Version:", e),
	}

	match &ready.npm_version_check {
		Ok(version) => println!("{:<17} {}", "NPM Version:", version),
		Err(e) => println!("{:<17} {}", "NPM Version:", e),
	}

	match &ready.cache_path_check {
		Ok(path) => println!("{:<17} {}", "Cache Path:", path.display()),
		Err(e) => println!("{:<17} {}", "Cache Path:", e),
	}

	match &ready.config_path_check {
		Ok(path) => println!("{:<17} {}", "Config Path:", path.display()),
		Err(e) => println!("{:<17} {}", "Config Path:", e),
	}

	match &ready.data_path_check {
		Ok(path) => println!("{:<17} {}", "Data Path:", path.display()),
		Err(e) => println!("{:<17} {}", "Data Path:", e),
	}

	match &ready.github_token_check {
		Ok(_) => println!("{:<17} Found!", "GitHub Token:"),
		Err(e) => println!("{:<17} {}", "GitHub Token:", e),
	}

	if ready.is_ready() {
		println!("Hipcheck is ready to run!");
	} else {
		println!("Hipheck is NOT ready to run");
	}
}

/// Print the current home directory for Hipcheck.
///
/// Exits `Ok` if home directory is specified, `Err` otherwise.
fn cmd_print_home(path: Option<&Path>) {
	match path.ok_or_else(|| hc_error!("can't find cache directory")) {
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
	match config_path.ok_or_else(|| hc_error!("can't find config directory")) {
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
	match data_path.ok_or_else(|| hc_error!("can't find data directory")) {
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
				Err(x) => {
					return (
						session.end(),
						Err(x), // Error::msg("Trouble scoring and analyzing results")),
					);
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
