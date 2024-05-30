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
use crate::analysis::session::resolve_config;
use crate::analysis::session::resolve_data;
use crate::analysis::session::resolve_home;
use crate::analysis::session::Check;
use crate::analysis::session::CheckType;
use crate::analysis::session::Session;
use crate::cli::Commands;
use crate::command_util::DependentProgram;
use crate::context::Context as _;
use crate::error::Error;
use crate::error::Result;
use crate::shell::ColorChoice;
use crate::shell::Output;
use crate::shell::Shell;
use crate::shell::Verbosity;
use crate::util::iter::TryAny;
use crate::util::iter::TryFilter;
use clap::Parser as _;
use cli::CheckCommand;
use cli::HelpCommand;
use cli::SchemaCommand;
use dotenv::var;
use env_logger::Builder;
use env_logger::Env;
use schemars::schema_for;
use std::env;
use std::ffi::OsString;
use std::path::Path;
use std::path::PathBuf;
use std::process::exit;
use std::str::FromStr;

/// Entry point for Hipcheck.
///
/// Sets up logging and makes sure error codes are output correctly.
fn main() {
	init_log();
	exit(go().exit_code())
}

/// The environment variable for configuring logging output.
static LOG_NAME: &str = "HC_LOG";

/// The environment variable for configuring logging style.
static LOG_STYLE: &str = "HC_LOG_STYLE";

const MAVEN: &str = CheckKind::Maven.name();
const NPM: &str = CheckKind::Npm.name();
const PATCH: &str = CheckKind::Patch.name();
const PYPI: &str = CheckKind::Pypi.name();
const REPO: &str = CheckKind::Repo.name();
const REQUEST: &str = CheckKind::Request.name();
const SPDX: &str = CheckKind::Spdx.name();

/// Initialize the logger.
fn init_log() {
	let env = Env::new().filter(LOG_NAME).write_style(LOG_STYLE);
	Builder::from_env(env).init();
}

fn go() -> Outcome {
	// Get the source specifier and output directory from the user.
	let args = match CliArgs::from_env().context("argument parsing failed") {
		Ok(args) => args,
		Err(e) => {
			print_error(&e);
			return Outcome::Err;
		}
	};

	let raw_version = env!("CARGO_PKG_VERSION", "can't find Hipcheck package version");

	if args.check.check_type == CheckType::RepoSource
		|| args.check.check_type == CheckType::PrUri
		|| args.check.check_type == CheckType::PackageVersion
		|| args.check.check_type == CheckType::SpdxDocument
	{
		let (shell, report) = run(
			Output::stdout(args.color_choice),
			Output::stderr(args.color_choice),
			args.verbosity,
			args.check,
			args.config_path,
			args.data_path,
			args.home_dir,
			args.format,
			raw_version,
		);

		let mut stdout = Output::stdout(args.color_choice);
		let result = match report {
			Ok(AnyReport::Report(report)) => shell.report(&mut stdout, report, args.format),
			Ok(AnyReport::PrReport(pr_report)) => {
				shell.pr_report(&mut stdout, pr_report, args.format)
			}
			Err(e) => Err(e),
		};

		match result {
			Ok(_) => Outcome::Ok,
			Err(e) => {
				if shell.error(&e, args.format).is_err() {
					print_error(&e);
				}
				Outcome::Err
			}
		}
	} else {
		print_missing();
	}
}

/// The arguments passed to the program from the CLI.
struct CliArgs {
	/// The path to the configuration file.
	config_path: Option<PathBuf>,

	/// The path to the data folder.
	data_path: Option<PathBuf>,

	/// The path to the home/root directory.
	home_dir: Option<PathBuf>,

	/// The source specifier (local or remote).
	check: Check,

	/// How verbose the output should be.
	verbosity: Verbosity,

	/// Whether the output should use color.
	color_choice: ColorChoice,

	/// The format to use when reporting results.
	format: Format,
}

impl CliArgs {
	/// Pull arguments from the environment, potentially exiting with a help or version message.
	fn from_env() -> Result<CliArgs> {
		let args = cli::Args::parse();

		if args.extra_help {
			print_help();
		}

		if args.version {
			print_version();
		}

		let verbosity = Verbosity::from(args.verbosity);

		let home_dir = {
			let path: Option<&String> = args.home.as_ref();
			path.map(PathBuf::from)
		};

		// PANIC: Optional but has a default value, so unwrap() should never panic.
		let color_choice = {
			let color: &String = &args.color.unwrap();
			ColorChoice::from_str(color).unwrap()
		};

		let config_path = {
			let config: Option<&String> = args.config.as_ref();
			config.map(PathBuf::from)
		};

		let data_path = {
			let data: Option<&String> = args.data.as_ref();
			data.map(PathBuf::from)
		};

		let format = Format::use_json(args.json);

		// initialized later when the "check" subcommand is called
		let check;

		match args.command {
			Some(Commands::Help(args)) => match args.command {
				None => print_help(),
				Some(HelpCommand::Check) => print_check_help(),
				Some(HelpCommand::Schema) => print_schema_help(),
			},
			Some(Commands::Ready) => {
				print_readiness(
					home_dir.as_deref(),
					config_path.as_deref(),
					data_path.as_deref(),
				);
			}
			Some(Commands::Check(args)) => {
				if args.extra_help {
					print_check_help();
				}
				match args.command {
					Some(CheckCommand::Maven(args)) => {
						check = Check {
							check_type: CheckType::PackageVersion,
							check_value: OsString::from(args.package),
							parent_command_value: MAVEN.to_string(),
						}
					}
					Some(CheckCommand::Npm(args)) => {
						check = Check {
							check_type: CheckType::PackageVersion,
							check_value: OsString::from(args.package),
							parent_command_value: NPM.to_string(),
						}
					}
					Some(CheckCommand::Patch(args)) => {
						check = Check {
							check_type: CheckType::PatchUri,
							check_value: OsString::from(args.patch_file_uri),
							parent_command_value: PATCH.to_string(),
						}
					}
					Some(CheckCommand::Pypi(args)) => {
						check = Check {
							check_type: CheckType::PackageVersion,
							check_value: OsString::from(args.package),
							parent_command_value: PYPI.to_string(),
						}
					}
					Some(CheckCommand::Repo(args)) => {
						check = Check {
							check_type: CheckType::RepoSource,
							check_value: OsString::from(args.source),
							parent_command_value: REPO.to_string(),
						}
					}
					Some(CheckCommand::Request(args)) => {
						check = Check {
							check_type: CheckType::PrUri,
							check_value: OsString::from(args.pr_mr_uri),
							parent_command_value: REQUEST.to_string(),
						}
					}
					Some(CheckCommand::Spdx(args)) => {
						check = Check {
							check_type: CheckType::SpdxDocument,
							check_value: OsString::from(args.filepath),
							parent_command_value: SPDX.to_string(),
						}
					}
					None => print_check_help(),
				}
			}
			Some(Commands::Schema(args)) => {
				if args.extra_help {
					print_schema_help();
				}
				match args.command {
					Some(SchemaCommand::Maven) => print_maven_schema(),
					Some(SchemaCommand::Npm) => print_npm_schema(),
					Some(SchemaCommand::Patch) => print_patch_schema(),
					Some(SchemaCommand::Pypi) => print_pypi_schema(),
					Some(SchemaCommand::Repo) => print_report_schema(),
					Some(SchemaCommand::Request) => print_request_schema(),
					None => print_schema_help(),
				}
			}
			None => print_help(),
		}

		Ok(CliArgs {
			config_path,
			data_path,
			home_dir,
			check,
			verbosity,
			color_choice,
			format,
		})
	}
}

/// Global flags and options that are repeated in different help text.
const GLOBAL_HELP: &str = "\
FLAGS:
    -V, --version         print version information

OPTIONS (CONFIGURATION):
    -c, --config <FILE>   path to the configuration file [default: ./Hipcheck.toml]
    -d, --data <DIR>      path to the data folder, which includes the custom module_deps.js
    -H, --home <DIR>      set hipcheck home via command flag

OPTIONS (OUTPUT):
    -j, --json            output results in JSON format
    -k, --color <COLOR>   set output coloring ('always', 'never', or 'auto') [default: auto]
    -q, --quiet           silence progress reporting
";

/// Print Hipcheck's help text.
fn print_help() -> ! {
	let raw_version = env!("CARGO_PKG_VERSION", "can't find Hipcheck package version");

	let help_text = format!(
		"\
{} {}
{}

USAGE:
{} [FLAGS] [OPTIONS] [<TASK>]

TASKS:
    check <SUBTASK>       analyzes a repository or pull/merge request
    ready                 print a report of whether or not Hipcheck is ready to run (experimental)
    schema <SUBTASK>      print the schema for JSON-format output for a specified subtarget
    help [<SUBTASK>]      print help information, optionally for a given subcommand

{}",
		env!("CARGO_PKG_NAME"),
		version::get_version(raw_version).unwrap(),
		env!("CARGO_PKG_DESCRIPTION"),
		env!("CARGO_BIN_NAME"),
		GLOBAL_HELP
	);

	println!("{}", help_text);
	exit(Outcome::Err.exit_code());
}

/// Print the help text for Hipcheck's schema subcommand.
fn print_schema_help() -> ! {
	let raw_version = env!("CARGO_PKG_VERSION", "can't find Hipcheck package version");

	let help_text = format!(
		"\
{} {}
Print the schema for JSON-format output for a specified subtarget

USAGE:
{} [FLAGS] [OPTIONS] schema <SUBTASK>

SUBTASKS:
    repo                  print the schema for JSON-format output for running Hipcheck against a repository
    request               print the schema for JSON-format output for running Hipcheck against a pull/merge request
    patch                 print the schema for JSON-format output for running Hipcheck against a patch

{}",
		env!("CARGO_PKG_NAME"),
		version::get_version(raw_version).unwrap(),
		env!("CARGO_BIN_NAME"),
		GLOBAL_HELP
	);

	println!("{}", help_text);
	exit(Outcome::Err.exit_code());
}

/// Print the help text for Hipcheck's check subcommand.
fn print_check_help() -> ! {
	let raw_version = env!("CARGO_PKG_VERSION", "can't find Hipcheck package version");

	let help_text = format!(
		"\
{} {}
Analyze a repository, pull/merge request, or 'git' patch

USAGE:
    {} [FLAGS] [OPTIONS] check <SUBTASK>

SUBTASKS:
    maven   <PACKAGE>     analyze a maven package git repo via package uri
    npm     <PACKAGE>     analyze an npm package git repo via package uri or with format <package name>[@<optional version>]
    patch   <PATCH URI>   analyze 'git' patches for projects that use a patch-based workflow (not yet implemented)
    pypi    <PACKAGE>     analyze a pypi package git repo via package uri or with format <package name>[@<optional version>]
    repo    <SOURCE>      analyze a repository and output an overall risk assessment
    request <PR/MR URI>   analyze pull/merge request for potential risks
    spdx    <FILEPATH>    analyze packages specified in an SPDX document (not yet implemented)

{}",
		env!("CARGO_PKG_NAME"),
		version::get_version(raw_version).unwrap(),
		env!("CARGO_BIN_NAME"),
		GLOBAL_HELP
	);

	println!("{}", help_text);
	exit(Outcome::Err.exit_code());
}

/// Print the current version of Hipcheck.
fn print_version() -> ! {
	let raw_version = env!("CARGO_PKG_VERSION", "can't find Hipcheck package version");

	let version_text = format!(
		"{} {}",
		env!("CARGO_PKG_NAME"),
		version::get_version(raw_version).unwrap()
	);
	println!("{}", version_text);
	exit(Outcome::Err.exit_code());
}

/// Print the JSON schema of the report.
fn print_report_schema() -> ! {
	let schema = schema_for!(Report);
	let report_text = serde_json::to_string_pretty(&schema).unwrap();
	println!("{}", report_text);
	exit(Outcome::Err.exit_code());
}

/// Print the JSON schema of the pull/merge request.
fn print_request_schema() -> ! {
	let schema = schema_for!(PrReport);
	let report_text = serde_json::to_string_pretty(&schema).unwrap();
	println!("{}", report_text);
	exit(Outcome::Err.exit_code());
}

/// Print the JSON schem a of the maven package
fn print_maven_schema() -> ! {
	print_missing()
}

/// Print the JSON schem a of the npm package
fn print_npm_schema() -> ! {
	print_missing()
}

/// Print the JSON schem a of the patch.
fn print_patch_schema() -> ! {
	print_missing()
}

/// Print the JSON schem a of the pypi package
fn print_pypi_schema() -> ! {
	print_missing()
}

/// Prints a "readiness report" for Hipcheck, indicating if any dependent programs are missing or our of date
/// or if any necessary folders or files are missing. It then returns a final readiness status.
fn print_readiness(
	home_dir: Option<&Path>,
	config_path: Option<&Path>,
	data_path: Option<&Path>,
) -> ! {
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
	let git_check = data::git::get_git_version();
	match git_check {
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
	let npm_check = data::npm::get_npm_version();
	match npm_check {
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
	let hipcheck_home = resolve_home(home_dir);
	match hipcheck_home {
		Ok(path_buffer) => println!("Hipcheck home directory: {}", path_buffer.display()),
		Err(err) => {
			failed = true;
			print_error(&err);
		}
	}

	// Check that the Hipcheck config TOML exists in the designated location
	let hipcheck_config = resolve_config(config_path);
	match hipcheck_config {
		Ok(path_buffer) => println!("Hipcheck config file: {}", path_buffer.display()),
		Err(err) => {
			failed = true;
			print_error(&err);
		}
	}

	// Check that Hipcheck data folder is findable
	let hipcheck_data = resolve_data(data_path);
	match hipcheck_data {
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
	match var("HC_GITHUB_TOKEN") {
		Ok(_) => println!("HC_GITHUB_TOKEN system environment variable found."),
		Err(_) => println!("Missing HC_GITHUB_TOKEN system environment variable. Some analyses will not run without this token set."),
	}

	if failed {
		println!("One or more dependencies or configuration settings are missing. Hipcheck is not ready to run.");
	} else {
		println!("Hipcheck is ready to run!");
	}

	let exit_code = Outcome::Err.exit_code();
	exit(exit_code);
}

/// Prints a message telling the user that this functionality has not been implemented yet
fn print_missing() -> ! {
	println!("This feature is not implemented yet.");
	let exit_code = Outcome::Ok.exit_code();
	exit(exit_code)
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

//used in hc_session::pm and main.rs, global variables for hc check CheckKindHere node-ipc@9.2.1
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
		&check.check_value,
		config_path,
		data_path,
		home_dir,
		format,
		raw_version,
	) {
		Ok(session) => session,
		Err((shell, err)) => return (shell, Err(err)),
	};

	match check.check_type {
		CheckType::RepoSource | CheckType::SpdxDocument => {
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
		CheckType::PackageVersion => {
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
		CheckType::PrUri => {
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

enum Outcome {
	Ok,
	Err,
}

impl Outcome {
	fn exit_code(&self) -> i32 {
		match self {
			Outcome::Ok => 0,
			Outcome::Err => 1,
		}
	}
}
