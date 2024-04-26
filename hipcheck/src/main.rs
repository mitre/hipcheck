// SPDX-License-Identifier: Apache-2.0

use clap::{App, AppSettings, Arg, SubCommand};
use env_logger::{Builder, Env};
use hc_common::{
	common::Context, error::Result, hc_error, schemars::schema_for, serde_json, CheckKind,
};
use hc_core::{
	print_error, resolve_config, resolve_data, resolve_home, run, version, AnyReport, ColorChoice,
	Format, Outcome, Output, PrReport, Report, Verbosity,
};
use hc_session::{Check, CheckType};
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::exit;
use std::str::FromStr;
use std::{env, path::Path};

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
	let args = match Args::from_env().context("argument parsing failed") {
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
struct Args {
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

impl Args {
	/// Pull arguments from the environment, potentially exiting with a help or version message.
	fn from_env() -> Result<Args> {
		let matches = App::new("Hipcheck")
			.about("Automatically assess and score git repositories for risk.")
			.version(get_version().as_ref())
			.setting(AppSettings::DisableHelpSubcommand)
			.subcommand(
				SubCommand::with_name("help")
					.setting(AppSettings::DisableHelpSubcommand)
					.subcommand(
						SubCommand::with_name("check").setting(AppSettings::DisableHelpFlags),
					)
					.subcommand(
						SubCommand::with_name("schema").setting(AppSettings::DisableHelpFlags),
					),
			)
			.arg(
				Arg::with_name("extra_help")
					.short("h")
					.long("help")
					.help("print help text"),
			)
			.arg(
				Arg::with_name("version")
					.short("V")
					.long("version")
					.help("print version information")
					.global(true),
			)
			.subcommand(
				SubCommand::with_name("schema")
					.help("print schema for JSON-format output")
					.setting(AppSettings::DisableHelpSubcommand)
					.arg(
						Arg::with_name("extra_help")
							.short("h")
							.long("help")
							.help("print help text")
							.global(true),
					)
					.subcommand(SubCommand::with_name(MAVEN))
					.subcommand(SubCommand::with_name(NPM))
					.subcommand(SubCommand::with_name(PATCH))
					.subcommand(SubCommand::with_name(PYPI))
					.subcommand(SubCommand::with_name(REPO))
					.subcommand(SubCommand::with_name(REQUEST)),
			)
			.arg(
				Arg::with_name("print home")
					.long("print-home")
					.help("print the home directory for Hipcheck")
					.global(true),
			)
			.arg(
				Arg::with_name("print config")
					.long("print-config")
					.help("print the config file path for Hipcheck")
					.global(true),
			)
			.arg(
				Arg::with_name("print data")
					.long("print-data")
					.help("print the data folder path for Hipcheck")
					.global(true),
			)
			.arg(
				Arg::with_name("verbosity")
					.short("q")
					.long("quiet")
					.help("silence progress reporting")
					.global(true),
			)
			.arg(
				Arg::with_name("color")
					.short("k")
					.long("color")
					.value_name("COLOR")
					.help("set output coloring ('always', 'never', or 'auto')")
					.takes_value(true)
					// Defaults to "auto"
					.default_value("auto")
					.global(true),
			)
			.arg(
				Arg::with_name("config")
					.short("c")
					.long("config")
					.value_name("FILE")
					.help("path to the configuration file")
					.takes_value(true)
					.global(true),
			)
			.arg(
				Arg::with_name("data")
					.short("d")
					.long("data")
					.value_name("FILE")
					.help("path to the data folder")
					.takes_value(true)
					.global(true),
			)
			.arg(
				Arg::with_name("home")
					.short("H")
					.long("home")
					.value_name("FILE")
					.help("path to the hipcheck home")
					.takes_value(true)
					.global(true),
			)
			.arg(
				Arg::with_name("json")
					.short("j")
					.long("json")
					.help("output results in JSON format")
					.global(true),
			)
			.subcommand(
				SubCommand::with_name("check")
					.setting(AppSettings::DisableHelpSubcommand)
					.arg(
						Arg::with_name("extra_help")
							.short("h")
							.long("help")
							.help("print help text")
							.global(true),
					)
					.subcommand(
						SubCommand::with_name(REPO).arg(
							Arg::with_name("source")
								.value_name("SOURCE")
								.help("repository to analyze; can be a local path or a URI")
								.index(1)
								.required(true),
						),
					)
					.subcommand(
						SubCommand::with_name(REQUEST).arg(
							Arg::with_name("PR/MR URI")
								.value_name("PR/MR URI")
								.help("URI of pull/merge request to analyze")
								.index(1)
								.required(true),
						),
					)
					.subcommand(
						SubCommand::with_name(MAVEN).arg(
							Arg::with_name("PACKAGE")
								.value_name("PACKAGE")
								.help("maven package uri to analyze")
								.index(1)
								.required(true),
						),
					)
					.subcommand(
						SubCommand::with_name(NPM).arg(
							Arg::with_name("PACKAGE")
								.value_name("PACKAGE")
								.help("npm package uri or package[@<optional version>] to analyze")
								.index(1)
								.required(true),
						),
					)
					.subcommand(
						SubCommand::with_name(PATCH).arg(
							Arg::with_name("patch file URI")
								.value_name("PATCH FILE URI")
								.help("URI of git patch to analyze")
								.index(1)
								.required(true),
						),
					)
					.subcommand(
						SubCommand::with_name(PYPI).arg(
							Arg::with_name("PACKAGE")
								.value_name("PACKAGE")
								.help("pypi package uri or package[@<optional version>] to analyze")
								.index(1)
								.required(true),
						),
					)
					.subcommand(
						SubCommand::with_name("spdx").arg(
							Arg::with_name("filepath")
								.value_name("FILEPATH")
								.help("SPDX document to analyze")
								.index(1)
								.required(true),
						),
					),
			)
			.get_matches();

		if matches.is_present("extra_help") {
			print_help();
		}

		if matches.is_present("version") {
			print_version();
		}

		let verbosity = Verbosity::from(matches.is_present("verbosity"));

		let home_dir = matches.value_of("home").map(PathBuf::from);

		if matches.is_present("print home") {
			print_home(home_dir.as_deref());
		}

		// PANIC: Optional but has a default value, so unwrap() should never panic.
		let color_choice = ColorChoice::from_str(matches.value_of("color").unwrap()).unwrap();

		let config_path = matches.value_of("config").map(PathBuf::from);

		if matches.is_present("print config") {
			print_config(config_path.as_deref());
		}

		let data_path = matches.value_of("data").map(PathBuf::from);

		if matches.is_present("print data") {
			print_data(data_path.as_deref());
		}

		let format = Format::use_json(matches.is_present("json"));

		// initialized later when the "check" subcommand is called
		let check;

		match matches.subcommand() {
			("help", Some(sub_help)) => {
				match sub_help.subcommand_name() {
					None => print_help(),
					Some("schema") => print_schema_help(),
					Some("check") => print_check_help(),
					_ => print_help(),
				};
			}
			("schema", Some(sub_schema)) => {
				if sub_schema.is_present("extra_help") {
					print_schema_help();
				}
				match sub_schema.subcommand_name() {
					Some(REPO) => print_report_schema(),
					Some(REQUEST) => print_request_schema(),
					Some(PATCH) => print_patch_schema(),
					Some(NPM) => print_npm_schema(),
					Some(MAVEN) => print_maven_schema(),
					Some(PYPI) => print_pypi_schema(),
					_ => print_schema_help(),
				}
			}
			("check", Some(sub_check)) => {
				if sub_check.is_present("extra_help") {
					print_check_help();
				}
				match sub_check.subcommand() {
					(REPO, Some(repo_source)) => {
						check = Check {
							check_type: CheckType::RepoSource,
							check_value: OsString::from(
								repo_source
									.value_of("source")
									.ok_or_else(|| hc_error!("missing SOURCE in arguments"))?,
							),
							parent_command_value: REPO.to_string(),
						};
					}
					(REQUEST, Some(request_uri)) => {
						check = Check {
							check_type: CheckType::PrUri,
							check_value: OsString::from(
								request_uri
									.value_of("PR/MR URI")
									.ok_or_else(|| hc_error!("missing PR/MR URI in arguments"))?,
							),
							parent_command_value: REQUEST.to_string(),
						};
					}
					(MAVEN, Some(package_version)) => {
						check = Check {
							check_type: CheckType::PackageVersion,
							check_value: OsString::from(
								package_version.value_of("PACKAGE").ok_or_else(|| {
									hc_error!("missing maven package uri in arguments")
								})?,
							),
							parent_command_value: MAVEN.to_string(),
						};
					}
					(NPM, Some(package_version)) => {
						check = Check {
							check_type: CheckType::PackageVersion,
							check_value: OsString::from(
								package_version.value_of("PACKAGE").ok_or_else(|| {
									hc_error!("missing npm package uri or <package name>[@<optional version>] in arguments")
								})?,
							),
							parent_command_value: NPM.to_string(),
						};
					}
					(PYPI, Some(package_version)) => {
						check = Check {
							check_type: CheckType::PackageVersion,
							check_value: OsString::from(
								package_version.value_of("PACKAGE").ok_or_else(|| {
									hc_error!("missing pypi package uri or package[@<optional version>] in arguments")
								})?,
							),
							parent_command_value: PYPI.to_string(),
						};
					}
					(PATCH, Some(patch_uri)) => {
						check = Check {
							check_type: CheckType::PatchUri,
							check_value: OsString::from(
								patch_uri.value_of("patch file URI").ok_or_else(|| {
									hc_error!("missing PATCH FILE URI in arguments")
								})?,
							),
							parent_command_value: PATCH.to_string(),
						};
					}
					("spdx", Some(filepath)) => {
						check = Check {
							check_type: CheckType::SpdxDocument,
							check_value: OsString::from(
								filepath
									.value_of("filepath")
									.ok_or_else(|| hc_error!("missing FILEPATH in arguments"))?,
							),
							parent_command_value: SPDX.to_string(),
						};
					}
					(_, _) => print_check_help(),
				}
			}
			_ => print_help(),
		}

		Ok(Args {
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
    --print-config        print the config file path for Hipcheck
    --print-data          print the data folder path for Hipcheck
    --print-home          print the home directory for Hipcheck

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
    spdx    <FILEPATH>    analyze packages specified in an SPDX document

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

/// Get the current version of Hipcheck as a String.
fn get_version() -> String {
	let raw_version = env!("CARGO_PKG_VERSION", "can't find Hipcheck package version");

	let version_text = format!(
		"{} {}",
		env!("CARGO_PKG_NAME"),
		version::get_version(raw_version).unwrap()
	);

	version_text
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

/// Print the current home directory for Hipcheck.
///
/// Exits `Ok` if home directory is specified, `Err` otherwise.
fn print_home(path: Option<&Path>) -> ! {
	let hipcheck_home = resolve_home(path);

	let exit_code = match hipcheck_home {
		Ok(path_buffer) => {
			println!("{}", path_buffer.display());
			Outcome::Ok.exit_code()
		}
		Err(err) => {
			print_error(&err);
			Outcome::Err.exit_code()
		}
	};

	exit(exit_code);
}

/// Print the current config path for Hipcheck.
///
/// Exits `Ok` if config path is specified, `Err` otherwise.
fn print_config(config_path: Option<&Path>) -> ! {
	let hipcheck_config = resolve_config(config_path);

	let exit_code = match hipcheck_config {
		Ok(path_buffer) => {
			println!("{}", path_buffer.display());
			Outcome::Ok.exit_code()
		}
		Err(err) => {
			print_error(&err);
			Outcome::Err.exit_code()
		}
	};

	exit(exit_code);
}

/// Print the current data folder path for Hipcheck.
///
/// Exits `Ok` if config path is specified, `Err` otherwise.
fn print_data(data_path: Option<&Path>) -> ! {
	let hipcheck_data = resolve_data(data_path);

	let exit_code = match hipcheck_data {
		Ok(path_buffer) => {
			println!("{}", path_buffer.display());
			Outcome::Ok.exit_code()
		}
		Err(err) => {
			print_error(&err);
			Outcome::Err.exit_code()
		}
	};

	exit(exit_code);
}

/// Prints a message telling the user that this functionality has not been implemented yet
fn print_missing() -> ! {
	println!("This feature is not implemented yet.");
	let exit_code = Outcome::Ok.exit_code();
	exit(exit_code)
}
