// SPDX-License-Identifier: Apache-2.0

//! Data structures for Hipcheck's main CLI.

/// Automatically assess and score git repositories for risk
#[derive(Debug, clap::Parser)]
#[command(about, disable_help_flag=true, disable_version_flag=true, long_about=None)]
pub struct Args {
	/// print help text
	#[arg(short = 'h', long = "help")]
	pub extra_help: bool,

	/// print version information
	#[arg(short = 'V', long, global = true)]
	pub version: bool,

	/// print the home directory for Hipcheck
	#[arg(long = "print-home", global = true)]
	pub print_home: bool,

	/// print the config file path for Hipcheck
	#[arg(long = "print-config", global = true)]
	pub print_config: bool,

	/// print the data folder path for Hipcheck
	#[arg(long = "print-data", global = true)]
	pub print_data: bool,

	/// silences progress reporting
	#[arg(short = 'q', long = "quiet", global = true)]
	pub verbosity: bool,

	/// set output coloring ('always', 'never', or 'auto')
	#[arg(
		short = 'k',
		long,
		default_value = "auto",
		value_name = "COLOR",
		global = true
	)]
	pub color: Option<String>,

	/// path to the configuration file
	#[arg(short, long, value_name = "FILE", global = true)]
	pub config: Option<String>,

	/// path to the data folder
	#[arg(short, long, value_name = "FILE", global = true)]
	pub data: Option<String>,

	/// path to the hipcheck home
	#[arg(short = 'H', long, value_name = "FILE", global = true)]
	pub home: Option<String>,

	/// output results in JSON format
	#[arg(short, long, global = true)]
	pub json: bool,

	#[command(subcommand)]
	pub command: Option<Commands>,
}

// impl Default for Args {
// 	fn default() -> Self {
// 		command::None
// 	}
// }

#[derive(Debug, clap::Subcommand)]
pub enum Commands {
	/// Analyzes a repository or pull/merge request
	#[command(disable_help_subcommand = true)]
	Check(CheckArgs),
	/// Print help information, optionally for a given subcommand
	Help(HelpArgs),
	/// Print the schema for JSON-format output for a specified subtarget
	#[command(disable_help_subcommand = true)]
	Schema(SchemaArgs),
}

impl Default for Commands {
	fn default() -> Commands {
		Commands::Help(HelpArgs { command: None })
	}
}

#[derive(Debug, clap::Args)]
pub struct CheckArgs {
	/// print help text
	#[arg(short = 'h', long = "help", global = true)]
	pub extra_help: bool,

	#[clap(subcommand)]
	pub command: Option<CheckCommand>,
}

#[derive(Debug, clap::Subcommand)]
pub enum CheckCommand {
	/// Analyze a maven package git repo via package URI
	Maven(CheckMavenArgs),
	/// Analyze an npm package git repo via package URI or with format <package name>[@<optional version>]
	Npm(CheckNpmArgs),
	/// Analyze 'git' patches for projects that use a patch-based workflow (not yet implemented)
	Patch(CheckPatchArgs),
	/// Analyze a PyPI package git repo via package URI or with format <package name>[@<optional version>]
	Pypi(CheckPypiArgs),
	/// Analyze a repository and output an overall risk assessment
	Repo(CheckRepoArgs),
	/// Analyze pull/merge request for potential risks
	Request(CheckRequestArgs),
	/// Analyze packages specified in an SPDX document
	Spdx(CheckSpdxArgs),
}

#[derive(Debug, clap::Args)]
pub struct CheckMavenArgs {
	/// Maven package URI to analyze
	#[arg(value_name = "PACKAGE", index = 1)]
	pub package: String,
}

#[derive(Debug, clap::Args)]
pub struct CheckNpmArgs {
	/// NPM package URI or package[@<optional version>] to analyze
	#[arg(value_name = "PACKAGE", index = 1)]
	pub package: String,
}

#[derive(Debug, clap::Args)]
pub struct CheckPatchArgs {
	/// URI of git patch to analyze
	#[arg(value_name = "PATCH FILE URI", index = 1)]
	pub patch_file_uri: String,
}

#[derive(Debug, clap::Args)]
pub struct CheckPypiArgs {
	/// PyPI package URI or package[@<optional version>] to analyze"
	#[arg(value_name = "PACKAGE", index = 1)]
	pub package: String,
}

#[derive(Debug, clap::Args)]
pub struct CheckRepoArgs {
	/// Repository to analyze; can be a local path or a URI
	#[arg(value_name = "SOURCE", index = 1)]
	pub source: String,
}

#[derive(Debug, clap::Args)]
pub struct CheckRequestArgs {
	/// URI of pull/merge request to analyze
	#[arg(value_name = "PR/MR URI", index = 1)]
	pub pr_mr_uri: String,
}

#[derive(Debug, clap::Args)]
pub struct CheckSpdxArgs {
	/// SPDX document to analyze
	#[arg(value_name = "FILEPATH", index = 1)]
	pub filepath: String,
}

#[derive(Debug, clap::Args)]
pub struct HelpArgs {
	#[clap(subcommand)]
	pub command: Option<HelpCommand>,
}

#[derive(Debug, clap::Subcommand)]
pub enum HelpCommand {
	/// Print help information for the check subcommand
	Check,
	/// Print help information for the schema subcommand
	Schema,
}

#[derive(Debug, clap::Args)]
pub struct SchemaArgs {
	/// print help text
	#[arg(short = 'h', long = "help", global = true)]
	pub extra_help: bool,

	#[clap(subcommand)]
	pub command: Option<SchemaCommand>,
}

#[derive(Debug, clap::Subcommand)]
pub enum SchemaCommand {
	/// Print the schema for JSON-format output for running Hipcheck against a Maven package
	Maven,
	/// Print the schema for JSON-format output for running Hipcheck against a NPM package
	Npm,
	/// Print the schema for JSON-format output for running Hipcheck against a patch
	Patch,
	/// Print the schema for JSON-format output for running Hipcheck against a PyPI package
	Pypi,
	/// Print the schema for JSON-format output for running Hipcheck against a repository
	Repo,
	/// Print the schema for JSON-format output for running Hipcheck against a pull/merge request
	Request,
}
