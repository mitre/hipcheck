// SPDX-License-Identifier: Apache-2.0

//! Data structures for Hipcheck's main CLI.

use crate::error::Error;
use crate::hc_error;
use crate::report::Format;
use crate::session::session::Check;
use crate::shell::{ColorChoice, Verbosity};
use crate::target::TargetType;
use crate::CheckKind;
use clap::{Parser as _, ValueEnum};
use hipcheck_macros as hc;
use pathbuf::pathbuf;
use std::path::{Path, PathBuf};

/// Automatated supply chain risk assessment of software packages.
#[derive(Debug, Default, clap::Parser, hc::Update)]
#[command(name = "Hipcheck", about, version, long_about=None, arg_required_else_help = true)]
pub struct CliConfig {
	#[command(subcommand)]
	command: Option<Commands>,

	/// Arguments configuring the CLI output.
	#[clap(flatten)]
	output_args: OutputArgs,

	/// Arguments setting paths which Hipcheck will use.
	#[clap(flatten)]
	path_args: PathArgs,

	/// Soft-deprecated flags.
	///
	/// The following are flags which still work, but are hidden from help text.
	/// The goal in the future would be to remove these with a version break.
	#[clap(flatten)]
	deprecated_args: DeprecatedArgs,
}

/// Arguments configuring Hipcheck's output.
#[derive(Debug, Default, clap::Args, hc::Update)]
struct OutputArgs {
	/// How verbose to be.
	#[arg(
		short = 'v',
		long = "verbosity",
		global = true,
		help_heading = "Output Flags",
		long_help = "How verbose to be. Can also be set with the `HC_VERBOSITY` environment variable"
	)]
	verbosity: Option<Verbosity>,

	/// When to use color.
	#[arg(
		short = 'k',
		long = "color",
		global = true,
		help_heading = "Output Flags",
		long_help = "When to use color. Can also be set with the `HC_COLOR` environment variable"
	)]
	color: Option<ColorChoice>,

	/// What format to use.
	#[arg(
		short = 'f',
		long = "format",
		global = true,
		help_heading = "Output Flags",
		long_help = "What format to use. Can also be set with the `HC_FORMAT` environment variable"
	)]
	format: Option<Format>,
}

/// Arguments configuring paths for Hipcheck to use.
#[derive(Debug, Default, clap::Args, hc::Update)]
struct PathArgs {
	/// Path to the configuration folder.
	#[arg(
		short = 'c',
		long = "config",
		global = true,
		help_heading = "Path Flags",
		long_help = "Path to the configuration folder. Can also be set with the `HC_CONFIG` environment variable"
	)]
	config: Option<PathBuf>,

	/// Path to the data folder.
	#[arg(
		short = 'd',
		long = "data",
		global = true,
		help_heading = "Path Flags",
		long_help = "Path to the data folder. Can also be set with the `HC_DATA` environment variable"
	)]
	data: Option<PathBuf>,

	/// Path to the cache folder.
	#[arg(
		short = 'C',
		long = "cache",
		global = true,
		help_heading = "Path Flags",
		long_help = "Path to the cache folder. Can also be set with the `HC_CACHE` environment variable"
	)]
	cache: Option<PathBuf>,
}

/// Soft-deprecated arguments, to be removed in a future version.
#[derive(Debug, Default, clap::Args, hc::Update)]
struct DeprecatedArgs {
	/// Set quiet output.
	#[arg(short = 'q', long = "quiet", hide = true, global = true)]
	quiet: Option<bool>,

	/// Use JSON output format.
	#[arg(short = 'j', long = "json", hide = true, global = true)]
	json: Option<bool>,

	/// Print the home directory for Hipcheck.
	#[arg(long = "print-home", hide = true, global = true)]
	print_home: Option<bool>,

	/// Print the config file path for Hipcheck.
	#[arg(long = "print-config", hide = true, global = true)]
	print_config: Option<bool>,

	/// Print the data folder path for Hipcheck.
	#[arg(long = "print-data", hide = true, global = true)]
	print_data: Option<bool>,

	/// Path to the Hipcheck home folder.
	#[arg(short = 'H', long = "home", hide = true, global = true)]
	home: Option<PathBuf>,
}

impl CliConfig {
	/// Load CLI configuration.
	///
	/// This loads values in increasing order of precedence:
	///
	/// - Platform-specific defaults
	/// - Environment variables, if set
	/// - CLI flags, if set.
	/// - Final defaults, if still unset.
	pub fn load() -> CliConfig {
		let mut config = CliConfig::empty();
		config.update(&CliConfig::backups());
		config.update(&CliConfig::from_platform());
		config.update(&CliConfig::from_env());
		config.update(&CliConfig::from_cli());
		config
	}

	/// Get the selected subcommand, if any.
	pub fn subcommand(&self) -> Option<FullCommands> {
		if self.print_data() {
			return Some(FullCommands::PrintData);
		}

		if self.print_home() {
			return Some(FullCommands::PrintCache);
		}

		if self.print_config() {
			return Some(FullCommands::PrintConfig);
		}

		self.command.as_ref().map(FullCommands::from)
	}

	/// Get the configured verbosity.
	pub fn verbosity(&self) -> Verbosity {
		match (self.output_args.verbosity, self.deprecated_args.quiet) {
			(None, None) => Verbosity::default(),
			(None, Some(quiet)) => Verbosity::use_quiet(quiet),
			(Some(verbosity), None) => verbosity,
			(Some(verbosity), Some(_quiet)) => {
				log::warn!("verbosity specified with both -v/--verbosity and -q/--quiet; prefer -v/--verbosity");
				verbosity
			}
		}
	}

	/// Get the configured color.
	pub fn color(&self) -> ColorChoice {
		self.output_args.color.unwrap_or_default()
	}

	/// Get the configured format.
	pub fn format(&self) -> Format {
		match (self.output_args.format, self.deprecated_args.json) {
			(None, None) => Format::default(),
			(None, Some(json)) => Format::use_json(json),
			(Some(format), None) => format,
			(Some(format), Some(_json)) => {
				log::warn!(
					"format specified with both -f/--format and -j/--json; prefer -f/--format"
				);
				format
			}
		}
	}

	/// Get the path to the configuration directory.
	pub fn config(&self) -> Option<&Path> {
		self.path_args.config.as_deref()
	}

	/// Get the path to the data directory.
	pub fn data(&self) -> Option<&Path> {
		self.path_args.data.as_deref()
	}

	/// Get the path to the cache directory.
	pub fn cache(&self) -> Option<&Path> {
		match (&self.path_args.cache, &self.deprecated_args.home) {
			(None, None) => None,
			(None, Some(home)) => Some(home),
			(Some(cache), None) => Some(cache),
			(Some(cache), Some(_home)) => {
				log::warn!("cache directory specified with both -C/--cache and -H/--home; prefer -C/--cache");
				Some(cache)
			}
		}
	}

	/// Check if the `--print-home` flag was used.
	pub fn print_home(&self) -> bool {
		self.deprecated_args.print_home.unwrap_or(false)
	}

	/// Check if the `--print-config` flag was used.
	pub fn print_config(&self) -> bool {
		self.deprecated_args.print_config.unwrap_or(false)
	}

	/// Check if the `--print-data` flag was used.
	pub fn print_data(&self) -> bool {
		self.deprecated_args.print_data.unwrap_or(false)
	}

	/// Get an empty configuration object with nothing set.
	///
	/// This is just an alias for `default()`.
	fn empty() -> CliConfig {
		CliConfig::default()
	}

	/// Load configuration from CLI flags and positional arguments.
	///
	/// This is just an alias for `parse()`.
	fn from_cli() -> CliConfig {
		CliConfig::parse()
	}

	/// Load config from environment variables.
	///
	/// Note that this only loads _some_ config items from the environment.
	fn from_env() -> CliConfig {
		CliConfig {
			output_args: OutputArgs {
				verbosity: hc_env_var_value_enum("verbosity"),
				color: hc_env_var_value_enum("color"),
				format: hc_env_var_value_enum("format"),
			},
			path_args: PathArgs {
				config: hc_env_var("config"),
				data: hc_env_var("data"),
				cache: hc_env_var("cache"),
			},
			deprecated_args: DeprecatedArgs {
				home: hc_env_var("home"),
				..Default::default()
			},
			..Default::default()
		}
	}

	/// Load config from platform-specific information.
	///
	/// Note that this only loads _some_ config items based on platform-specific
	/// information.
	fn from_platform() -> CliConfig {
		CliConfig {
			path_args: PathArgs {
				cache: platform_cache(),
				config: platform_config(),
				data: platform_data(),
			},
			..Default::default()
		}
	}

	/// Set configuration backups for paths.
	fn backups() -> CliConfig {
		CliConfig {
			path_args: PathArgs {
				config: dirs::home_dir().map(|dir| pathbuf![&dir, "hipcheck", "config"]),
				data: dirs::home_dir().map(|dir| pathbuf![&dir, "hipcheck", "data"]),
				cache: dirs::home_dir().map(|dir| pathbuf![&dir, "hipcheck", "cache"]),
			},
			..Default::default()
		}
	}
}

/// Get the platform cache directory.
///
/// See: https://docs.rs/dirs/latest/dirs/fn.cache_dir.html
fn platform_cache() -> Option<PathBuf> {
	dirs::cache_dir().map(|dir| pathbuf![&dir, "hipcheck"])
}

/// Get the platform config directory.
///
/// See: https://docs.rs/dirs/latest/dirs/fn.config_dir.html
fn platform_config() -> Option<PathBuf> {
	let base = dirs::config_dir().map(|dir| pathbuf![&dir, "hipcheck"]);

	// Config and data paths aren't differentiated on MacOS or Windows,
	// so on those platforms we differentiate them ourselves.
	if cfg!(target_os = "macos") || cfg!(target_os = "windows") {
		base.map(|dir| pathbuf![&dir, "config"])
	} else {
		base
	}
}

/// Get the platform data directory.
///
/// See: https://docs.rs/dirs/latest/dirs/fn.data_dir.html
fn platform_data() -> Option<PathBuf> {
	let base = dirs::data_dir().map(|dir| pathbuf![&dir, "hipcheck"]);

	// Config and data paths aren't differentiated on MacOS or Windows,
	// so on those platforms we differentiate them ourselves.
	if cfg!(target_os = "macos") || cfg!(target_os = "windows") {
		base.map(|dir| pathbuf![&dir, "data"])
	} else {
		base
	}
}

/// Get a Hipcheck configuration environment variable.
///
/// This is generic in the return type, to automatically handle
/// converting from any type that can be derived from an [`OsString`].
fn hc_env_var<O: From<String>>(name: &'static str) -> Option<O> {
	let name = format!("HC_{}", name.to_uppercase());
	let val = dotenv::var(name).ok()?;
	Some(O::from(val))
}

/// Get a Hipcheck configuration environment variable and parse it into a [`ValueEnum`] type.
fn hc_env_var_value_enum<E: ValueEnum>(name: &'static str) -> Option<E> {
	let s: String = hc_env_var(name)?;

	// We don't ignore case; must be fully uppercase.
	let ignore_case = false;
	E::from_str(&s, ignore_case).ok()
}

/// All commands, both subcommands and flag-like commands.
pub enum FullCommands {
	Check(CheckArgs),
	Schema(SchemaArgs),
	Setup(SetupArgs),
	Ready,
	PrintConfig,
	PrintData,
	PrintCache,
}

impl From<&Commands> for FullCommands {
	fn from(command: &Commands) -> Self {
		match command {
			Commands::Check(args) => FullCommands::Check(args.clone()),
			Commands::Schema(args) => FullCommands::Schema(args.clone()),
			Commands::Setup(args) => FullCommands::Setup(args.clone()),
			Commands::Ready => FullCommands::Ready,
		}
	}
}

#[derive(Debug, Clone, clap::Subcommand)]
pub enum Commands {
	/// Analyze a package, source repository, SBOM, or pull request.
	Check(CheckArgs),
	/// Print the JSON schema for output of a specific `check` command.
	Schema(SchemaArgs),
	/// Initialize Hipcheck config file and script file locations.
	Setup(SetupArgs),
	/// Check if Hipcheck is ready to run.
	Ready,
}

// If no subcommand matched, default to use of '-t <TYPE> <TARGET' syntax. In
// this case, `target` is a required field, but the existence of a subcommand
// removes that requirement
#[derive(Debug, Clone, clap::Args)]
#[command(subcommand_negates_reqs = true)]
#[command(arg_required_else_help = true)]
pub struct CheckArgs {
	#[clap(subcommand)]
	command: Option<CheckCommand>,

	#[arg(short = 't', long = "target")]
	pub target_type: Option<TargetType>,
	#[arg(
		required = true,
		help = "The target package, URL, commit, etc. for Hipcheck to analyze. If ambiguous, the -t flag must be set"
	)]
	pub target: Option<String>,
	#[arg(trailing_var_arg(true), hide = true)]
	pub trailing_args: Vec<String>,
}

impl CheckArgs {
	fn target_to_check_command(&self) -> Result<CheckCommand, Error> {
		// Get target str
		let Some(target) = self.target.clone() else {
			return Err(hc_error!(
				"a target must be provided. The CLI should have caught this"
			));
		};
		// If a target type was provided use that, otherwise try to resolve from
		// the target string
		let opt_subcmd = self
			.target_type
			.clone()
			.or_else(|| TargetType::try_resolve_from_target(target.as_str()));
		let Some(subcmd) = opt_subcmd else {
			return Err(hc_error!(
				"could not resolve target '{}' to a target type. please specify with the `-t` flag",
				target
			));
		};
		// We have resolved the subcommand type. Re-construct a string with all args
		// that we can feed back into clap
		let binding = "check".to_owned();
		let subcmd_str = subcmd.as_str();
		let mut reconst_args: Vec<&String> = vec![&binding, &subcmd_str, &target];
		reconst_args.extend(self.trailing_args.iter());

		CheckCommand::try_parse_from(reconst_args).map_err(|e| hc_error!("{}", e))
	}

	pub fn command(&self) -> Result<CheckCommand, Error> {
		if let Some(cmd) = self.command.clone() {
			Ok(cmd)
		} else {
			self.target_to_check_command()
		}
	}
}

#[derive(Debug, Clone, clap::Parser)]
pub enum CheckCommand {
	/// Analyze a maven package git repo via package URI
	#[command(hide = true)]
	Maven(CheckMavenArgs),
	/// Analyze an npm package git repo via package URI or with format <package name>[@<optional version>]
	#[command(hide = true)]
	Npm(CheckNpmArgs),
	/// Analyze 'git' patches for projects that use a patch-based workflow (not yet implemented)
	#[command(hide = true)]
	Patch(CheckPatchArgs),
	/// Analyze a PyPI package git repo via package URI or with format <package name>[@<optional version>]
	#[command(hide = true)]
	Pypi(CheckPypiArgs),
	/// Analyze a repository and output an overall risk assessment
	#[command(hide = true)]
	Repo(CheckRepoArgs),
	/// Analyze pull/merge request for potential risks
	#[command(hide = true)]
	Request(CheckRequestArgs),
	/// Analyze packages specified in an SPDX document
	#[command(hide = true)]
	Spdx(CheckSpdxArgs),
}

impl CheckCommand {
	pub fn as_check(&self) -> Check {
		match self {
			CheckCommand::Maven(args) => Check {
				target: args.package.clone(),
				kind: CheckKind::Maven,
			},
			CheckCommand::Npm(args) => Check {
				target: args.package.clone(),
				kind: CheckKind::Npm,
			},
			CheckCommand::Patch(args) => Check {
				target: args.patch_file_uri.clone(),
				kind: CheckKind::Patch,
			},
			CheckCommand::Pypi(args) => Check {
				target: args.package.clone(),
				kind: CheckKind::Pypi,
			},
			CheckCommand::Repo(args) => Check {
				target: args.source.clone(),
				kind: CheckKind::Repo,
			},
			CheckCommand::Request(args) => Check {
				target: args.pr_mr_uri.clone(),
				kind: CheckKind::Request,
			},
			CheckCommand::Spdx(args) => Check {
				target: args.path.clone(),
				kind: CheckKind::Spdx,
			},
		}
	}
}

#[derive(Debug, Clone, clap::Args)]
pub struct CheckMavenArgs {
	/// Maven package URI to analyze
	pub package: String,
}

#[derive(Debug, Clone, clap::Args)]
pub struct CheckNpmArgs {
	/// NPM package URI or package[@<optional version>] to analyze
	pub package: String,
}

#[derive(Debug, Clone, clap::Args)]
pub struct CheckPatchArgs {
	/// Path to Git patch file to analyze
	#[arg(value_name = "PATCH FILE URI")]
	pub patch_file_uri: String,
}

#[derive(Debug, Clone, clap::Args)]
pub struct CheckPypiArgs {
	/// PyPI package URI or package[@<optional version>] to analyze"
	pub package: String,
}

#[derive(Debug, Clone, clap::Args)]
pub struct CheckRepoArgs {
	/// Repository to analyze; can be a local path or a URI
	pub source: String,
}

#[derive(Debug, Clone, clap::Args)]
pub struct CheckRequestArgs {
	/// URI of pull/merge request to analyze
	#[arg(value_name = "PR/MR URI")]
	pub pr_mr_uri: String,
}

#[derive(Debug, Clone, clap::Args)]
pub struct CheckSpdxArgs {
	/// SPDX document to analyze
	pub path: String,
}

#[derive(Debug, Clone, clap::Args)]
pub struct SchemaArgs {
	#[clap(subcommand)]
	pub command: SchemaCommand,
}

#[derive(Debug, Clone, clap::Subcommand)]
pub enum SchemaCommand {
	/// Print the JSON schema for running Hipcheck against a Maven package
	Maven,
	/// Print the JSON schema for running Hipcheck against a NPM package
	Npm,
	/// Print the JSON schema for running Hipcheck against a Git patchfile
	Patch,
	/// Print the JSON schema for running Hipcheck against a PyPI package
	Pypi,
	/// Print the JSON schema for running Hipcheck against a source repository
	Repo,
	/// Print the JSON schema for running Hipcheck against a pull request
	Request,
}

#[derive(Debug, Clone, clap::Args)]
pub struct SetupArgs {
	/// Setup will not attempt to pull missing files from the Hipcheck remote repository.
	#[clap(long, short)]
	pub offline: bool,
	/// Path to a local Hipcheck release archive or directory from which to copy default
	/// config and data dirs.
	pub source: Option<PathBuf>,
}

/// A type that can copy non-`None` values from other instances of itself.
pub trait Update {
	/// Update self with the value from other, if present.
	fn update(&mut self, other: &Self);
}

impl<T: Clone> Update for Option<T> {
	fn update(&mut self, other: &Option<T>) {
		if other.is_some() {
			self.clone_from(other);
		}
	}
}

/// Test CLI commands
#[cfg(test)]
mod tests {
	use super::*;
	use crate::cli::CliConfig;
	use crate::test_util::with_env_vars;
	use clap::CommandFactory;
	use tempfile::TempDir;

	const TEMPDIR_PREFIX: &str = "hipcheck";

	#[test]
	fn verify_cli() {
		CliConfig::command().debug_assert()
	}

	#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
	#[test]
	fn resolve_cache_with_platform() {
		let tempdir = TempDir::with_prefix(TEMPDIR_PREFIX).unwrap();

		let vars = vec![
			("HOME", Some(tempdir.path().to_str().unwrap())),
			("XDG_CACHE_HOME", None),
			("HC_CACHE", None),
		];

		with_env_vars(vars, || {
			let config = {
				let mut temp = CliConfig::empty();
				temp.update(&CliConfig::from_platform());
				temp.update(&CliConfig::from_env());
				temp
			};

			assert_eq!(config.cache().unwrap(), platform_cache().unwrap());
		});
	}

	#[test]
	fn resolve_cache_with_env_var() {
		let tempdir = TempDir::with_prefix(TEMPDIR_PREFIX).unwrap();

		let vars = vec![
			("HOME", None),
			("XDG_CACHE_HOME", None),
			("HC_CACHE", Some(tempdir.path().to_str().unwrap())),
		];

		with_env_vars(vars, || {
			let config = {
				let mut temp = CliConfig::empty();
				temp.update(&CliConfig::from_platform());
				temp.update(&CliConfig::from_env());
				temp
			};

			assert_eq!(config.cache().unwrap(), tempdir.path());
		});
	}

	#[test]
	fn resolve_cache_with_flag() {
		let tempdir = TempDir::with_prefix(TEMPDIR_PREFIX).unwrap();

		let vars = vec![
			("HOME", Some(tempdir.path().to_str().unwrap())),
			("XDG_CACHE_HOME", None),
			("HC_CACHE", None),
		];

		with_env_vars(vars, || {
			let expected = pathbuf![tempdir.path(), "hipcheck"];

			let config = {
				let mut temp = CliConfig::empty();
				temp.update(&CliConfig::from_platform());
				temp.update(&CliConfig::from_env());
				temp.update(&CliConfig {
					path_args: PathArgs {
						cache: Some(expected.clone()),
						..Default::default()
					},
					..Default::default()
				});
				temp
			};

			assert_eq!(config.cache().unwrap(), expected);
		});
	}

	#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
	#[test]
	fn resolve_config_with_platform() {
		let tempdir = TempDir::with_prefix(TEMPDIR_PREFIX).unwrap();

		let vars = vec![
			("HOME", Some(tempdir.path().to_str().unwrap())),
			("XDG_CONFIG_HOME", None),
			("HC_CONFIG", None),
		];

		with_env_vars(vars, || {
			let config = {
				let mut temp = CliConfig::empty();
				temp.update(&CliConfig::from_platform());
				temp.update(&CliConfig::from_env());
				temp
			};

			assert_eq!(config.config().unwrap(), platform_config().unwrap());
		});
	}

	#[test]
	fn resolve_config_with_env_var() {
		let tempdir = TempDir::with_prefix(TEMPDIR_PREFIX).unwrap();

		let vars = vec![
			("HOME", None),
			("XDG_CONFIG_HOME", None),
			("HC_CONFIG", Some(tempdir.path().to_str().unwrap())),
		];

		with_env_vars(vars, || {
			let config = {
				let mut temp = CliConfig::empty();
				temp.update(&CliConfig::from_platform());
				temp.update(&CliConfig::from_env());
				temp
			};

			assert_eq!(config.config().unwrap(), tempdir.path());
		});
	}

	#[test]
	fn resolve_config_with_flag() {
		let tempdir = TempDir::with_prefix(TEMPDIR_PREFIX).unwrap();

		let vars = vec![
			("HOME", Some(tempdir.path().to_str().unwrap())),
			("XDG_CONFIG_HOME", None),
			("HC_CONFIG", None),
		];

		with_env_vars(vars, || {
			let expected = pathbuf![tempdir.path(), "hipcheck"];

			let config = {
				let mut temp = CliConfig::empty();
				temp.update(&CliConfig::from_platform());
				temp.update(&CliConfig::from_env());
				temp.update(&CliConfig {
					path_args: PathArgs {
						config: Some(expected.clone()),
						..Default::default()
					},
					..Default::default()
				});
				temp
			};

			assert_eq!(config.config().unwrap(), expected);
		});
	}

	#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
	#[test]
	fn resolve_data_with_platform() {
		let tempdir = TempDir::with_prefix(TEMPDIR_PREFIX).unwrap();

		let vars = vec![
			("HOME", Some(tempdir.path().to_str().unwrap())),
			("XDG_DATA_HOME", None),
			("HC_DATA", None),
		];

		with_env_vars(vars, || {
			let config = {
				let mut temp = CliConfig::empty();
				temp.update(&CliConfig::from_platform());
				temp.update(&CliConfig::from_env());
				temp
			};

			assert_eq!(config.data().unwrap(), platform_data().unwrap());
		});
	}

	#[test]
	fn resolve_data_with_env_var() {
		let tempdir = TempDir::with_prefix(TEMPDIR_PREFIX).unwrap();

		let vars = vec![
			("HOME", None),
			("XDG_DATA_HOME", None),
			("HC_DATA", Some(tempdir.path().to_str().unwrap())),
		];

		with_env_vars(vars, || {
			let config = {
				let mut temp = CliConfig::empty();
				temp.update(&CliConfig::from_platform());
				temp.update(&CliConfig::from_env());
				temp
			};

			assert_eq!(config.data().unwrap(), tempdir.path());
		});
	}

	#[test]
	fn resolve_data_with_flag() {
		let tempdir = TempDir::with_prefix(TEMPDIR_PREFIX).unwrap();

		let vars = vec![
			("HOME", Some(tempdir.path().to_str().unwrap())),
			("XDG_DATA_HOME", None),
			("HC_DATA", None),
		];

		with_env_vars(vars, || {
			let expected = pathbuf![tempdir.path(), "hipcheck"];

			let config = {
				let mut temp = CliConfig::empty();
				temp.update(&CliConfig::from_platform());
				temp.update(&CliConfig::from_env());
				temp.update(&CliConfig {
					path_args: PathArgs {
						data: Some(expected.clone()),
						..Default::default()
					},
					..Default::default()
				});
				temp
			};

			assert_eq!(config.data().unwrap(), expected);
		});
	}

	#[test]
	fn hc_check_schema_no_args_gives_help() {
		let check_args = vec!["hc", "check"];
		let schema_args = vec!["hc", "schema"];

		let parsed = CliConfig::try_parse_from(check_args.into_iter());
		assert!(parsed.is_err());
		assert_eq!(
			parsed.unwrap_err().kind(),
			clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
		);

		let parsed = CliConfig::try_parse_from(schema_args.into_iter());
		assert!(parsed.is_err());
		assert_eq!(
			parsed.unwrap_err().kind(),
			clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
		);
	}

	fn get_check_cmd_from_cli(args: Vec<&str>) -> Result<CheckCommand, Error> {
		let parsed = CliConfig::try_parse_from(args.into_iter());
		assert!(parsed.is_ok());
		let command = parsed.unwrap().command;
		let Some(Commands::Check(chck_args)) = command else {
			assert!(false);
			unreachable!();
		};
		chck_args.command()
	}

	#[test]
	fn test_deprecated_check_repo() {
		let cmd = get_check_cmd_from_cli(vec![
			"hc",
			"check",
			"repo",
			"https://github.com/mitre/hipcheck.git",
		]);
		assert!(matches!(cmd, Ok(CheckCommand::Repo(..))));
	}

	#[test]
	fn test_deductive_check_no_match() {
		let cmd = get_check_cmd_from_cli(vec!["hc", "check", "pkg:unsupportedtype/someurl"]);
		assert!(matches!(cmd, Err(..)));
	}

	#[test]
	fn test_deductive_check_github_url() {
		let cmd =
			get_check_cmd_from_cli(vec!["hc", "check", "https://github.com/mitre/hipcheck.git"]);
		assert!(matches!(cmd, Ok(CheckCommand::Repo(..))));
	}

	#[test]
	fn test_deductive_check_maven_pkg() {
		let cmd = get_check_cmd_from_cli(vec![
			"hc",
			"check",
			"pkg:maven/org.apache.xmlgraphics/batik-anim@1.9.1",
		]);
		assert!(matches!(cmd, Ok(CheckCommand::Maven(..))));
	}

	#[test]
	fn test_check_with_target_flag() {
		let cmd = get_check_cmd_from_cli(vec![
			"hc",
			"check",
			"-t",
			"repo",
			"https://github.com/mitre/hipcheck.git",
		]);
		assert!(matches!(cmd, Ok(CheckCommand::Repo(..))));
	}
}
