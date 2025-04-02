// SPDX-License-Identifier: Apache-2.0

//! Data structures for Hipcheck's main CLI.

use crate::{
	cache::{
		plugin::{PluginCacheDeleteScope, PluginCacheListScope, PluginCacheSort},
		repo::{RepoCacheDeleteScope, RepoCacheListScope, RepoCacheSort},
	},
	error::{Context, Result},
	hc_error,
	plugin::Arch,
	shell::{color_choice::ColorChoice, verbosity::Verbosity},
	source,
	target::{
		pm, LocalGitRepo, MavenPackage, Package, PackageHost, Sbom, SbomStandard, SingleTargetSeed,
		SingleTargetSeedKind, TargetSeed, TargetSeedKind, TargetType, ToTargetSeed,
		ToTargetSeedKind, VcsUrl,
	},
};
use clap::{Parser as _, ValueEnum};
use hipcheck_macros as hc;
use pathbuf::pathbuf;
use semver::VersionReq;
use std::{
	fmt::{self, Display, Formatter},
	path::{Path, PathBuf},
	str::FromStr,
};
use url::Url;

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

/// Provenance container. Tracks whether a configuration was explicitly set by a
/// user, or is from Hipcheck default settings.
/// Could be made generic, but currently only works for PathBuf.
#[derive(Debug, Clone)]
enum Provenance {
	FromDefaults(PathBuf),
	FromUser(PathBuf),
}

impl std::ops::Deref for Provenance {
	type Target = Path;

	fn deref(&self) -> &Self::Target {
		use Provenance::*;
		match self {
			FromDefaults(path) => path,
			FromUser(path) => path,
		}
	}
}

impl FromStr for Provenance {
	type Err = core::convert::Infallible;

	fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
		// Assume that if we are parsing a string, that it was
		// configuration specified by a user directly.
		Ok(Self::FromUser(PathBuf::from(s)))
	}
}

/// Arguments configuring paths for Hipcheck to use.
#[derive(Debug, Default, clap::Args, hc::Update)]
struct PathArgs {
	/// Path to the cache folder.
	#[arg(
		short = 'C',
		long = "cache",
		global = true,
		help_heading = "Path Flags",
		long_help = "Path to the cache folder. Can also be set with the `HC_CACHE` environment variable"
	)]
	cache: Option<PathBuf>,

	/// Path to the policy file
	#[arg(
		short = 'p',
		long = "policy",
		global = true,
		help_heading = "Path Flags",
		long_help = "Path to the policy file."
	)]
	policy: Option<Provenance>,

	/// Path to the exec config file
	#[arg(
		short = 'e',
		long = "exec",
		global = true,
		help_heading = "Path Flags",
		long_help = "Path to the execution config file."
	)]
	exec: Option<PathBuf>,
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

	/// Path to the configuration folder.
	#[arg(short = 'c', long = "config", hide = true, global = true)]
	config: Option<Provenance>,

	/// Path to the Hipcheck home folder.
	#[arg(short = 'H', long = "home", hide = true, global = true)]
	home: Option<PathBuf>,
}

/// Select how Hipcheck searches for its configuration.
/// Hipcheck supports a legacy configuration format called config TOML.
/// The default system is called a Policy file, in KDL format.
/// If a user explicitly selects one of these modes, then Hipcheck
/// will only attempt to use that source of configuration.
/// Otherwise, Hipcheck will try to use a Policy file, falling back
/// to config TOML if that fails.
#[derive(Debug, Clone, PartialEq)]
pub enum ConfigMode {
	/// Try to load policy first, but fall back to config.
	PreferPolicy { policy: PathBuf, config: PathBuf },
	/// Only attempt to load from policy.
	ForcePolicy { policy: PathBuf },
	/// Only attempt to load from config.
	ForceConfig { config: PathBuf },
}

impl Display for ConfigMode {
	fn fmt(&self, f: &mut Formatter) -> fmt::Result {
		use ConfigMode::*;
		match &self {
			PreferPolicy { policy, config } => {
				write!(f, "Default to Policy KDL file at path:\n{:?}\nFallback Legacy Config TOML directory at path:\n{:?}", policy, config)
			}
			ForcePolicy { policy } => {
				write!(f, "Policy KDL file at path:\n{:?}", policy)
			}
			ForceConfig { config } => {
				write!(f, "Legacy Config TOML directory at path:\n{:?}", config)
			}
		}
	}
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

	/// Determine which ConfigMode to use, based on the combination of options
	/// passed to Hipcheck.
	pub fn config_mode(&self) -> Result<ConfigMode> {
		match (&self.path_args.policy, &self.deprecated_args.config) {
			(Some(Provenance::FromUser(policy_path)), _) => Ok(ConfigMode::ForcePolicy {
				policy: policy_path.to_path_buf(),
			}),
			(_, Some(Provenance::FromUser(config_path))) => Ok(ConfigMode::ForceConfig {
				config: config_path.to_path_buf(),
			}),
			(Some(Provenance::FromDefaults(policy_path)), Some(Provenance::FromDefaults(config_path))) => Ok(
				ConfigMode::PreferPolicy {
					policy: policy_path.to_path_buf(),
					config: config_path.to_path_buf(),
				}
			),
			_ => Err(hc_error!("Could not find any source of configuration. Use --policy or --config to configure Hipcheck."))
		}
	}

	/// Get the selected subcommand, if any.
	pub fn subcommand(&self) -> Option<FullCommands> {
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

	/// Get the path to the policy file.
	pub fn policy(&self) -> Option<&Path> {
		self.path_args.policy.as_deref()
	}

	/// Get the path to the configuration directory.
	pub fn config(&self) -> Option<&Path> {
		self.deprecated_args.config.as_deref()
	}

	/// Get the path to the exec config file
	pub fn exec(&self) -> Option<&Path> {
		self.path_args.exec.as_deref()
	}

	/// Check if the `--print-home` flag was used.
	pub fn print_home(&self) -> bool {
		self.deprecated_args.print_home.unwrap_or(false)
	}

	/// Check if the `--print-config` flag was used.
	pub fn print_config(&self) -> bool {
		self.deprecated_args.print_config.unwrap_or(false)
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
				cache: hc_env_var("cache"),
				// For now, we do not get this from the environment, so pass a None to never update this field
				policy: None,
				// For now, we don't get this from the environment
				exec: None,
			},
			deprecated_args: DeprecatedArgs {
				config: hc_env_var("config").map(Provenance::FromUser),
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
				policy: platform_config()
					.map(|dir| Provenance::FromDefaults(pathbuf![&dir, "Hipcheck.kdl"])),
				exec: None,
			},
			deprecated_args: DeprecatedArgs {
				config: platform_config().map(Provenance::FromDefaults),
				..Default::default()
			},
			..Default::default()
		}
	}

	/// Set configuration backups for paths.
	fn backups() -> CliConfig {
		CliConfig {
			path_args: PathArgs {
				cache: dirs::home_dir().map(|dir| pathbuf![&dir, "hipcheck", "cache"]),
				policy: std::env::current_dir()
					.ok()
					.map(|dir| Provenance::FromDefaults(pathbuf![&dir, "Hipcheck.kdl"])),
				exec: None,
			},
			deprecated_args: DeprecatedArgs {
				config: dirs::home_dir()
					.map(|dir| Provenance::FromDefaults(pathbuf![&dir, "hipcheck", "config"])),
				..Default::default()
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

	// Config and (now unused) data paths aren't differentiated on MacOS or Windows,
	// so on those platforms we differentiate them ourselves.
	if cfg!(target_os = "macos") || cfg!(target_os = "windows") {
		base.map(|dir| pathbuf![&dir, "config"])
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
	Setup,
	Ready(ReadyArgs),
	Update(UpdateArgs),
	CacheTarget(CacheTargetArgs),
	CachePlugin(CachePluginArgs),
	Plugin(PluginArgs),
	PrintConfig,
	PrintCache,
	Scoring,
	ExplainTargetTriple,
}

impl From<&Commands> for FullCommands {
	fn from(command: &Commands) -> Self {
		match command {
			Commands::Check(args) => FullCommands::Check(args.clone()),
			Commands::Schema(args) => FullCommands::Schema(args.clone()),
			Commands::Setup => FullCommands::Setup,
			Commands::Ready(args) => FullCommands::Ready(args.clone()),
			Commands::Scoring => FullCommands::Scoring,
			Commands::Update(args) => FullCommands::Update(args.clone()),
			Commands::Cache(args) => match &args.subcmd {
				CacheSubcmds::Target(cache_target_args) => {
					FullCommands::CacheTarget(cache_target_args.clone())
				}

				CacheSubcmds::Plugin(cache_plugin_args) => {
					FullCommands::CachePlugin(cache_plugin_args.clone())
				}
			},
			Commands::Plugin(args) => FullCommands::Plugin(args.clone()),
			Commands::Explain(args) => match &args.subcmd {
				ExplainSubcmds::TargetTriple => FullCommands::ExplainTargetTriple,
			},
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
	Setup,
	/// Check if Hipcheck is ready to run.
	Ready(ReadyArgs),
	/// Print the tree used to weight analyses during scoring.
	Scoring,
	/// Run Hipcheck self-updater, if installed
	Update(UpdateArgs),
	/// Manage Hipcheck repo or plugin cache
	Cache(CacheArgs),
	Plugin(PluginArgs),
	/// View setup information to help debug
	Explain(ExplainArgs),
}

// If no subcommand matched, default to use of '-t <TYPE> <TARGET' syntax. In
// this case, `target` is a required field, but the existence of a subcommand
// removes that requirement
#[derive(Debug, Clone, clap::Args)]
#[command(subcommand_negates_reqs = true)]
#[command(arg_required_else_help = true)]
pub struct CheckArgs {
	/// The ref (e.g. commit hash, branch, tag) of the target to analyze
	#[clap(long = "ref", value_name = "REF")]
	pub git_ref: Option<String>,

	#[clap(subcommand)]
	command: Option<CheckCommand>,

	#[arg(long = "arch", value_parser = Arch::from_str)]
	pub arch: Option<Arch>,

	#[arg(short = 't', long = "target")]
	pub target_type: Option<TargetType>,
	#[arg(
		required = true,
		help = "The target package, URL, commit, etc. for Hipcheck to analyze. If ambiguous, the -t flag must be set"
	)]
	pub target: Option<String>,
	#[arg(trailing_var_arg(true), allow_hyphen_values(true), hide = true)]
	pub trailing_args: Vec<String>,
}

impl CheckArgs {
	fn target_to_check_command(&self) -> Result<CheckCommand> {
		// Get target str
		let Some(target) = self.target.clone() else {
			return Err(hc_error!(
				"a target must be provided. The CLI should have caught this"
			));
		};

		let subcmd_str;
		let target_str;

		// Try to resolve the type by checking if the target string is a pURL, GitHub URL, or SBOM (SPDX or CycloneDX) file
		match TargetType::try_resolve_from_target(target.as_str()) {
			Some((subcmd, new_target)) => {
				subcmd_str = subcmd.as_str();
				// If the type had to be resolved from a pURL, the target string must be reformatted
				// Update that string here
				target_str = new_target;
				// Check if the user also provided a type, and error if it does not agree with the inferred type.
				if let Some(user_submcd) = self.target_type.clone() {
					if user_submcd.as_str() != subcmd_str {
						return Err(hc_error!(
							"Provided target type '{}' does not match the type, '{}', inferred from the target '{}'. Check that you have specified the correct type and provided the intended target.",
							user_submcd.as_str(), subcmd_str, target
						));
					}
				}
			}
			None => match self.target_type.clone() {
				// If a type could not be inferred, check if a type was provided
				Some(subcmd) => {
					subcmd_str = subcmd.as_str();
					// If a type was provided, use the provided target string
					target_str = target.clone();
				}
				// If no type was inferred or provided, return an error
				None => {
					return Err(hc_error!(
					"could not resolve target '{}' to a target type. please specify with the `-t` flag",
					target
				))
				}
			},
		}

		// We have resolved the subcommand type. Re-construct a string with all args
		// that we can feed back into clap
		let binding = "check".to_owned();
		let mut reconst_args: Vec<&String> = vec![&binding, &subcmd_str, &target_str];
		reconst_args.extend(self.trailing_args.iter());

		CheckCommand::try_parse_from(reconst_args).map_err(|e| hc_error!("{}", e))
	}

	pub fn command(&self) -> Result<CheckCommand> {
		if let Some(cmd) = self.command.clone() {
			Ok(cmd)
		} else {
			self.target_to_check_command()
		}
	}
}
impl ToTargetSeed for CheckArgs {
	fn to_target_seed(&mut self) -> Result<TargetSeed> {
		let command = self.command()?;

		match command.to_target_seed_kind()? {
			TargetSeedKind::Single(single_target_seed_kind) => {
				let mut refspec = self.git_ref.clone();
				// Validate refspec not contradicted by target ref information
				if let Some(init_ref) = &refspec {
					// Validate for package
					if let SingleTargetSeedKind::Package(p) = &single_target_seed_kind {
						if p.has_version() && &p.version != init_ref {
							return Err(hc_error!("ambiguous version for package target: package target specified {}, but refspec flag specified {}. please specify only one.", p.version, init_ref));
						}
					}
					// Validate for VCS URL
					else if let SingleTargetSeedKind::VcsUrl(vcs) = &single_target_seed_kind {
						if let Some(git_ref) = &vcs.git_ref {
							if git_ref != init_ref {
								return Err(hc_error!(
								"Provided ref_spec '{}' does not match the ref spec, '{}', inferred from the target '{}'. Check that you have specified the correct ref and provided the intended target.",
								init_ref, git_ref, &command.get_specifier()
							));
							}
						}
					}
				} else {
					// If no --ref is set and the target seed is a VCS URL, get the ref from that
					if let SingleTargetSeedKind::VcsUrl(vcs) = &single_target_seed_kind {
						refspec = vcs.git_ref.clone();
					}
				}

				let seed = SingleTargetSeed {
					kind: single_target_seed_kind,
					refspec: refspec.to_owned(),
					specifier: command.get_specifier().to_string(),
				};

				Ok(TargetSeed::Single(seed))
			}
			TargetSeedKind::Multi(_multi_target_seed_kind) => todo!(),
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
	/// Analyze a PyPI package git repo via package URI or with format <package name>[@<optional version>]
	#[command(hide = true)]
	Pypi(CheckPypiArgs),
	/// Analyze a repository and output an overall risk assessment
	#[command(hide = true)]
	Repo(CheckRepoArgs),
	/// Analyze packages specified in an SBOM document
	#[command(hide = true)]
	Sbom(CheckSbomArgs),
}

impl CheckCommand {
	fn get_specifier(&self) -> &str {
		use CheckCommand::*;
		match self {
			Maven(args) => &args.package,
			Npm(args) => &args.package,
			Pypi(args) => &args.package,
			Repo(args) => &args.source,
			Sbom(args) => &args.path,
		}
	}
}

impl ToTargetSeedKind for CheckCommand {
	fn to_target_seed_kind(&self) -> Result<TargetSeedKind> {
		match self {
			CheckCommand::Maven(args) => args.to_target_seed_kind(),
			CheckCommand::Npm(args) => args.to_target_seed_kind(),
			CheckCommand::Pypi(args) => args.to_target_seed_kind(),
			CheckCommand::Repo(args) => args.to_target_seed_kind(),
			CheckCommand::Sbom(args) => args.to_target_seed_kind(),
		}
	}
}

#[derive(Debug, Clone, clap::Args)]
pub struct CheckMavenArgs {
	/// Maven package URI to analyze
	pub package: String,
}

impl ToTargetSeedKind for CheckMavenArgs {
	fn to_target_seed_kind(&self) -> Result<TargetSeedKind> {
		let arg = &self.package;
		// Confirm that the provided URL is valid.
		let url = Url::parse(arg)
			.map_err(|e| hc_error!("The provided Maven URL {} is not a valid URL. {}", arg, e))?;
		Ok(TargetSeedKind::Single(SingleTargetSeedKind::MavenPackage(
			MavenPackage { url },
		)))
	}
}

#[derive(Debug, Clone, clap::Args)]
pub struct CheckNpmArgs {
	/// NPM package URI or package[@<optional version>] to analyze
	pub package: String,
}

impl ToTargetSeedKind for CheckNpmArgs {
	fn to_target_seed_kind(&self) -> Result<TargetSeedKind> {
		let raw_package = &self.package;

		let (name, version) = match Url::parse(raw_package) {
			Ok(url_parsed) => pm::extract_package_version_from_url(url_parsed)?,
			_ => pm::extract_package_version(raw_package)?,
		};

		let purl = Url::parse(&format_npm_url(&name, &version))
			.map_err(|e| hc_error!("Failed to parse package url: {}", e))?;

		Ok(TargetSeedKind::Single(SingleTargetSeedKind::Package(
			Package {
				purl,
				name,
				version,
				host: PackageHost::Npm,
			},
		)))
	}
}

pub fn format_npm_url(name: &str, version: &str) -> String {
	// If the package is scoped, replace the leading '@' in the scope with %40 for proper pURL formatting
	match version {
		"no version" => format!("pkg:npm/{}", str::replace(name, '@', "%40")),
		_ => format!("pkg:npm/{}@{}", str::replace(name, '@', "%40"), version),
	}
}

#[derive(Debug, Clone, clap::Args)]
pub struct CheckPypiArgs {
	/// PyPI package URI or package[@<optional version>] to analyze"
	pub package: String,
}

impl ToTargetSeedKind for CheckPypiArgs {
	fn to_target_seed_kind(&self) -> Result<TargetSeedKind> {
		let raw_package = &self.package;

		let (name, version) = match Url::parse(raw_package) {
			Ok(url_parsed) => pm::extract_package_version_from_url(url_parsed)?,
			_ => pm::extract_package_version(raw_package)?,
		};

		let purl = Url::parse(&match version.as_str() {
			"no version" => format!("pkg:pypi/{}", name),
			_ => format!("pkg:pypi/{}@{}", name, version),
		})
		.unwrap();

		Ok(TargetSeedKind::Single(SingleTargetSeedKind::Package(
			Package {
				purl,
				name,
				version,
				host: PackageHost::PyPI,
			},
		)))
	}
}

#[derive(Debug, Clone, clap::Args)]
pub struct CheckRepoArgs {
	/// Repository to analyze; can be a local path or a URI
	pub source: String,
}

impl ToTargetSeedKind for CheckRepoArgs {
	fn to_target_seed_kind(&self) -> Result<TargetSeedKind> {
		// First check if this is a git VCS URL
		if self.source.starts_with("git+") {
			let mut git_ref = None;
			// Remove git prefix
			let mut spec_trimmed = self.source.replace("git+", "");

			// We check that the VCS URL is valid when inferring a VCS URL target; this should only error when `hc check repo` is used
			let url = Url::parse(&spec_trimmed).context(format!(
				"Provided target repository {} is not a valid VCS URL",
				self.source
			))?;
			match url.scheme() {
				// If the URL is for a file, trim the file scheme identifier and usethe presumptive file path
				"file" => {
					let filepath = url.path().replace("file://", "");
					to_local_repo(&filepath)
				}
				_ => {
					// Remove any git ref information that trails the end of the URL and set the ref_spec equal to that ref
					if let Some((repo_url, vcs_refspec)) = spec_trimmed.split_once(".git@") {
						git_ref = Some(vcs_refspec.to_string());
						// Restore ".git" to the end of the URL, since we did not intend to remove that part
						spec_trimmed = format!("{repo_url}.git");
					}
					let remote = source::get_remote_repo_from_url(Url::parse(&spec_trimmed)?)?;
					Ok(TargetSeedKind::Single(SingleTargetSeedKind::VcsUrl(
						VcsUrl { remote, git_ref },
					)))
				}
			}
		// Next check if it is a repo URL
		} else if let Ok(url) = Url::parse(&self.source) {
			let remote_repo = source::get_remote_repo_from_url(url)?;
			Ok(TargetSeedKind::Single(SingleTargetSeedKind::RemoteRepo(
				remote_repo,
			)))
		// Otherwise treat it as local file
		} else {
			to_local_repo(&self.source)
		}
	}
}

fn to_local_repo(source: &String) -> Result<TargetSeedKind> {
	let path = Path::new(&source).canonicalize()?;
	if path.exists() {
		Ok(TargetSeedKind::Single(SingleTargetSeedKind::LocalRepo(
			LocalGitRepo {
				path,
				git_ref: "".to_owned(),
			},
		)))
	} else {
		Err(hc_error!("Provided target repository could not be identified as either a remote url or path to a local file"))
	}
}

#[derive(Debug, Clone, clap::Args)]
pub struct CheckSbomArgs {
	/// SPDX document to analyze
	pub path: String,
}

impl ToTargetSeedKind for CheckSbomArgs {
	fn to_target_seed_kind(&self) -> Result<TargetSeedKind> {
		let path = PathBuf::from(&self.path);
		if path.exists() {
			if self.path.ends_with(".spdx") {
				Ok(TargetSeedKind::Single(SingleTargetSeedKind::Sbom(Sbom {
					path,
					standard: SbomStandard::Spdx,
				})))
			} else if self.path.ends_with("bom.json")
				|| self.path.ends_with(".cdx.json")
				|| self.path.ends_with("bom.xml")
				|| self.path.ends_with(".cdx.xml")
			{
				Ok(TargetSeedKind::Single(SingleTargetSeedKind::Sbom(Sbom {
					path,
					standard: SbomStandard::CycloneDX,
				})))
			} else {
				Err(hc_error!(
					"The provided SBOM file is not an SPDX or CycloneDX file type"
				))
			}
		} else {
			Err(hc_error!("The provided SBOM file does not exist"))
		}
	}
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
	/// Print the JSON schema for running Hipcheck against a PyPI package
	Pypi,
	/// Print the JSON schema for running Hipcheck against a source repository
	Repo,
}

#[derive(Debug, Clone, clap::Args)]
pub struct ReadyArgs {
	#[arg(long = "arch", value_parser = Arch::from_str)]
	pub arch: Option<Arch>,
}

#[derive(Debug, Clone, clap::Args)]
pub struct UpdateArgs {
	/// Installs the specified tag instead of the latest version
	#[clap(long)]
	pub tag: Option<String>,
	/// Installs the specified version instead of the latest version
	#[clap(long)]
	pub version: Option<String>,
	/// Allows prereleases when just updating to "latest"
	#[clap(long)]
	pub prerelease: bool,
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
/// subcommands for cache // copy from here
#[derive(Debug, Clone, clap::Parser)]
#[command(arg_required_else_help = true)]
pub struct CacheArgs {
	#[clap(subcommand)]
	pub subcmd: CacheSubcmds,
}
#[derive(Debug, Clone, clap::Subcommand)]
#[command(arg_required_else_help = true)]
pub enum CacheSubcmds {
	/// Manipulates repo cache
	Target(CacheTargetArgs),
	/// Manipulates plugin cache
	Plugin(CachePluginArgs),
}
#[derive(Debug, Clone, clap::Parser)]
pub struct CacheTargetArgs {
	#[clap(subcommand)]
	pub subcmd: CacheTargetSubcmds,
}
impl TryFrom<CacheTargetArgs> for CacheOp {
	type Error = crate::error::Error;
	fn try_from(value: CacheTargetArgs) -> Result<Self> {
		value.subcmd.try_into()
	}
}

// The target struct to which a CacheTargetArgs instance must be translated
#[derive(Debug, Clone)]
pub enum CacheOp {
	List {
		scope: RepoCacheListScope,
		filter: Option<String>,
	},
	Delete {
		scope: RepoCacheDeleteScope,
		filter: Option<String>,
		force: bool,
	},
}

#[derive(Debug, Clone, clap::Subcommand)]
#[command(arg_required_else_help = true)]
pub enum CacheTargetSubcmds {
	/// List cached targets.
	List(CliCacheListArgs),
	/// Delete cached targets.
	Delete(CliCacheDeleteArgs),
}
impl TryFrom<CacheTargetSubcmds> for CacheOp {
	type Error = crate::error::Error;
	fn try_from(value: CacheTargetSubcmds) -> Result<Self> {
		use CacheTargetSubcmds::*;
		match value {
			List(args) => Ok(args.into()),
			Delete(args) => args.try_into(),
		}
	}
}

// CLI version of cache::CacheSort with `invert` field expanded to different
// named sort strategies
#[derive(Debug, Clone, clap::ValueEnum)]
pub enum CliSortStrategy {
	/// Oldest entries first
	Oldest,
	/// Newest entries first
	Newest,
	/// Largest entries first
	Largest,
	/// Smallest entries first
	Smallest,
	/// Entries sorted alphabetically
	Alpha,
	/// Entries sorted reverse-alphabetically
	Ralpha,
}
impl CliSortStrategy {
	pub fn to_cache_sort(&self) -> (RepoCacheSort, bool) {
		use CliSortStrategy::*;
		match self {
			Oldest => (RepoCacheSort::Oldest, false),
			Newest => (RepoCacheSort::Oldest, true),
			Largest => (RepoCacheSort::Largest, false),
			Smallest => (RepoCacheSort::Largest, true),
			Alpha => (RepoCacheSort::Alpha, false),
			Ralpha => (RepoCacheSort::Alpha, true),
		}
	}
}

// Args for `hc cache list`
#[derive(Debug, Clone, clap::Args)]
pub struct CliCacheListArgs {
	/// Sorting strategy for the list, default is 'alpha'
	#[arg(short = 's', long, default_value = "alpha")]
	pub strategy: CliSortStrategy,
	/// Max number of entries to display
	#[arg(short = 'm', long)]
	pub max: Option<usize>,
	/// Consider only entries matching this pattern
	#[arg(short = 'P', long = "pattern")]
	pub filter: Option<String>,
}
impl From<CliCacheListArgs> for CacheOp {
	fn from(value: CliCacheListArgs) -> Self {
		let (sort, invert) = value.strategy.to_cache_sort();
		let scope = RepoCacheListScope {
			sort,
			invert,
			n: value.max,
		};
		CacheOp::List {
			scope,
			filter: value.filter,
		}
	}
}

// Args for `hc cache delete`
#[derive(Debug, Clone, clap::Args)]
pub struct CliCacheDeleteArgs {
	/// Sorting strategy for deletion. Args of the form 'all|{<STRAT> [N]}'. Where <STRAT> is the
	/// same set of strategies for `hc cache list`. If [N], the max number of entries to delete is
	/// omitted, it will default to 1.
	#[arg(short = 's', long, num_args=1..=2, value_delimiter = ' ')]
	pub strategy: Vec<String>,
	/// Consider only entries matching this pattern
	#[arg(short = 'P', long = "pattern")]
	pub filter: Option<String>,
	/// Do not prompt user to confirm the entries to delete
	#[arg(long, default_value_t = false)]
	pub force: bool,
}
// Must be fallible conversion because we are doing validation that clap can't
// support as of writing
impl TryFrom<CliCacheDeleteArgs> for CacheOp {
	type Error = crate::error::Error;
	fn try_from(value: CliCacheDeleteArgs) -> Result<Self> {
		if value.strategy.is_empty() && value.filter.is_none() {
			return Err(hc_error!(
				"`hc cache target delete` without args is not allowed. please \
                tailor the operation with flags, or use `-s all` to delete all \
                entries"
			));
		}
		let scope: RepoCacheDeleteScope = value.strategy.try_into()?;
		Ok(CacheOp::Delete {
			scope,
			filter: value.filter,
			force: value.force,
		})
	}
}

// A valid cli string for CacheDeleteScope may be:
//  1. "all"
//  2. "<SORT> <N>", where <SORT> is one of the CliSortStrategy variants, <N> is
//      number of entries
//  3. "<SORT>", same as #2 but <N> defaults to 1
impl TryFrom<Vec<String>> for RepoCacheDeleteScope {
	type Error = crate::error::Error;
	fn try_from(value: Vec<String>) -> Result<Self> {
		if value.len() > 2 {
			return Err(hc_error!("strategy takes at most two tokens"));
		}
		let Some(raw_spec) = value.first() else {
			return Ok(RepoCacheDeleteScope::All);
		};
		if raw_spec.to_lowercase() == "all" {
			if let Some(n) = value.get(1) {
				return Err(hc_error!(
					"unnecessary additional token '{}' after 'all'",
					n
				));
			}
			Ok(RepoCacheDeleteScope::All)
		} else {
			let strat = CliSortStrategy::from_str(raw_spec, true).or(Err(hc_error!(
				"'{}' is not a valid sort strategy. strategies include {}, or 'all'",
				raw_spec,
				CliSortStrategy::value_variants()
					.iter()
					.map(|s| format!("'{s:?}'").to_lowercase())
					.collect::<Vec<String>>()
					.join(", "),
			)))?;
			let (sort, invert) = strat.to_cache_sort();
			let n: usize = match value.get(1) {
				Some(raw_n) => raw_n
					.parse::<usize>()
					.context("max entry token is not a valid unsigned int")?,
				None => 1,
			};
			Ok(RepoCacheDeleteScope::Group { sort, invert, n })
		}
	}
}
#[derive(Debug, Clone, clap::Parser)]
pub struct CachePluginArgs {
	#[clap(subcommand)]
	pub subcmd: CachePluginSubcmds,
}
impl TryFrom<CachePluginArgs> for PluginOp {
	type Error = crate::error::Error;
	fn try_from(value: CachePluginArgs) -> Result<Self> {
		value.subcmd.try_into()
	}
}
// The target struct to which a PluginTargetArgs instance must be translated
#[derive(Debug, Clone)]
pub enum PluginOp {
	List {
		scope: PluginCacheListScope,
		name: Option<String>,
		publisher: Option<String>,
		version: Option<VersionReq>,
	},
	Delete {
		scope: PluginCacheDeleteScope,
		name: Option<String>,
		publisher: Option<String>,
		version: Option<VersionReq>,
		force: bool,
	},
}
#[derive(Debug, Clone, clap::Subcommand)]
#[command(arg_required_else_help = true)]
pub enum CachePluginSubcmds {
	/// List cached plugins.
	List(CliPluginListArgs),
	/// Delete cached plugins.
	Delete(CliPluginDeleteArgs),
}

impl TryFrom<CachePluginSubcmds> for PluginOp {
	type Error = crate::error::Error;
	fn try_from(value: CachePluginSubcmds) -> Result<Self> {
		use CachePluginSubcmds::*;
		match value {
			List(args) => Ok(args.into()),
			Delete(args) => args.try_into(),
		}
	}
}
// CLI version of cache::PluginCacheSort with `invert` field expanded to different
// named sort strategies
#[derive(Debug, Clone, clap::ValueEnum)]
pub enum PluginSortStrategy {
	/// Oldest dated entries first
	Oldest,
	/// Newest dated entries first
	Newest,
	/// Entries sorted alphabetically
	Alpha,
	/// Entries sorted reverse-alphabetically
	Ralpha,
	/// Newest version entries first
	Latest,
	/// Oldest version entries first
	Rlatest,
}
impl PluginSortStrategy {
	pub fn to_plugin_sort(&self) -> (PluginCacheSort, bool) {
		use PluginSortStrategy::*;
		match self {
			Oldest => (PluginCacheSort::OldestDate, false),
			Newest => (PluginCacheSort::OldestDate, true),
			Alpha => (PluginCacheSort::Alpha, false),
			Ralpha => (PluginCacheSort::Alpha, true),
			Latest => (PluginCacheSort::LatestVersion, false),
			Rlatest => (PluginCacheSort::LatestVersion, true),
		}
	}
}
// Args for `hc cache list`
#[derive(Debug, Clone, clap::Args)]
pub struct CliPluginListArgs {
	/// Sorting strategy for the list, default is 'alpha'
	#[arg(short = 's', long, default_value = "alpha")]
	pub strategy: PluginSortStrategy,
	/// Filter plugins by their name.
	#[arg(short = 'N', long = "name")]
	pub name: Option<String>,
	/// Filter plugins by their publisher.
	#[arg(short = 'P', long = "publisher")]
	pub publisher: Option<String>,
	/// Filter plugins by a SemVer-compatible version requirement
	#[arg(short = 'V', long = "version")]
	pub version: Option<VersionReq>,
	/// Max number of entries to display
	#[arg(short = 'm', long)]
	pub max: Option<usize>,
}
impl From<CliPluginListArgs> for PluginOp {
	fn from(value: CliPluginListArgs) -> Self {
		let (sort, invert) = value.strategy.to_plugin_sort();
		let scope = PluginCacheListScope {
			sort,
			invert,
			n: value.max,
		};
		PluginOp::List {
			scope,
			name: value.name,
			publisher: value.publisher,
			version: value.version,
		}
	}
}
// Args for `hc cache plugin delete`
#[derive(Debug, Clone, clap::Args)]
pub struct CliPluginDeleteArgs {
	/// Sorting strategy for deletion. Args of the form 'all|{<STRAT> [N]}'. Where <STRAT> is the
	/// same set of strategies for `hc cache list`. If [N], the max number of entries to delete is
	/// omitted, it will default to 1.
	#[arg(short = 's', long, num_args=1..=2, value_delimiter = ' ')]
	pub strategy: Vec<String>,
	/// Consider only entries matching this name
	#[arg(short = 'N', long = "name")]
	pub name: Option<String>,
	/// Consider only entries matching this publisher.
	#[arg(short = 'P', long = "publisher")]
	pub publisher: Option<String>,
	/// Consider only entries matching this version requirement. Must be formatted as a SemVer-compatible version requirement (ex: ">= 0.3.2") or version number. If formatted as a version requirement, must be in quotation marks
	#[arg(short = 'V', long = "version")]
	pub version: Option<VersionReq>,
	/// Do not prompt user to confirm the entries to delete
	#[arg(long, default_value_t = false)]
	pub force: bool,
}
impl TryFrom<CliPluginDeleteArgs> for PluginOp {
	type Error = crate::error::Error;
	fn try_from(value: CliPluginDeleteArgs) -> Result<Self> {
		if value.strategy.is_empty()
			&& value.name.is_none()
			&& value.version.is_none()
			&& value.publisher.is_none()
		{
			return Err(hc_error!(
				"`hc cache plugin delete` without args is not allowed. please \
                tailor the operation with flags, or use `-s all` to delete all \
                entries"
			));
		}
		let scope: PluginCacheDeleteScope = value.strategy.try_into()?;
		Ok(PluginOp::Delete {
			scope,
			name: value.name,
			publisher: value.publisher,
			version: value.version,
			force: value.force,
		})
	}
}
// A valid cli string for Plugin CacheDeleteScope may be:
//  1. "all"
//  2. "<SORT> <N>", where <SORT> is one of the CliSortStrategy variants, <N> is
//      number of entries
//  3. "<SORT>", same as #2 but <N> defaults to 1
impl TryFrom<Vec<String>> for PluginCacheDeleteScope {
	type Error = crate::error::Error;
	fn try_from(value: Vec<String>) -> Result<Self> {
		if value.len() > 2 {
			return Err(hc_error!("strategy takes at most two tokens"));
		}
		let Some(raw_spec) = value.first() else {
			return Ok(PluginCacheDeleteScope::All);
		};
		if raw_spec.to_lowercase() == "all" {
			if let Some(n) = value.get(1) {
				return Err(hc_error!(
					"unnecessary additional token '{}' after 'all'",
					n
				));
			}
			Ok(PluginCacheDeleteScope::All)
		} else {
			let strat = PluginSortStrategy::from_str(raw_spec, true).or(Err(hc_error!(
				"'{}' is not a valid sort strategy. strategies include {}, or 'all'",
				raw_spec,
				PluginSortStrategy::value_variants()
					.iter()
					.map(|s| format!("'{s:?}'").to_lowercase())
					.collect::<Vec<String>>()
					.join(", "),
			)))?;
			let (sort, invert) = strat.to_plugin_sort();
			let n: usize = match value.get(1) {
				Some(raw_n) => raw_n
					.parse::<usize>()
					.context("max entry token is not a valid unsigned int")?,
				None => 1,
			};
			Ok(PluginCacheDeleteScope::Group { sort, invert, n })
		}
	}
}
#[derive(Debug, Clone, clap::Args)]
pub struct PluginArgs {
	#[arg(long = "async")]
	pub asynch: bool,
}
/// The format to report results in.
#[derive(Debug, Default, Clone, Copy, clap::ValueEnum)]
pub enum Format {
	/// JSON format.
	Json,
	/// JSON format with raw analysis values
	Debug,
	/// Human-readable format.
	#[default]
	Human,
}

impl Format {
	pub fn use_json(json: bool) -> Format {
		if json {
			Format::Json
		} else {
			Format::Human
		}
	}
}

// Help subcommand
#[derive(Debug, Clone, clap::Parser)]
#[command(arg_required_else_help = true)] // idk
pub struct ExplainArgs {
	#[clap(subcommand)]
	pub subcmd: ExplainSubcmds,
}
#[derive(Debug, Clone, clap::Subcommand)]
#[command(arg_required_else_help = true)]
pub enum ExplainSubcmds {
	/// Show the current and known architecture targets
	TargetTriple,
}

/// Test CLI commands
#[cfg(test)]
mod tests {
	use super::*;
	use crate::{cli::CliConfig, util::test::with_env_vars};
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
			let config_dir = platform_config().unwrap();
			let path = pathbuf![&config_dir, "Hipcheck.kdl"];
			let expected = ConfigMode::PreferPolicy {
				policy: path,
				config: config_dir,
			};

			let config = {
				let mut temp = CliConfig::empty();
				temp.update(&CliConfig::from_platform());
				temp.update(&CliConfig::from_env());
				temp
			};

			assert_eq!(config.config_mode().unwrap(), expected);
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
			let path = pathbuf![tempdir.path(), "hipcheck"];
			let expected = ConfigMode::ForceConfig {
				config: path.clone(),
			};

			let config = {
				let mut temp = CliConfig::empty();
				temp.update(&CliConfig::from_platform());
				temp.update(&CliConfig::from_env());
				temp.update(&CliConfig {
					deprecated_args: DeprecatedArgs {
						config: Some(Provenance::FromUser(path.clone())),
						..Default::default()
					},
					..Default::default()
				});
				temp
			};

			assert_eq!(config.config_mode().unwrap(), expected);
		});
	}

	#[test]
	fn resolve_policy_with_flag() {
		let tempdir = TempDir::with_prefix(TEMPDIR_PREFIX).unwrap();

		let path = pathbuf![tempdir.path(), "HipcheckPolicy.kdl"];
		let expected = ConfigMode::ForcePolicy {
			policy: path.clone(),
		};

		let config = {
			let mut temp = CliConfig::empty();
			temp.update(&CliConfig::from_platform());
			temp.update(&CliConfig::from_env());
			temp.update(&CliConfig {
				path_args: PathArgs {
					policy: Some(Provenance::FromUser(path.clone())),
					..Default::default()
				},
				..Default::default()
			});
			temp
		};

		assert_eq!(config.config_mode().unwrap(), expected);
	}

	#[test]
	fn hc_check_schema_no_args_gives_help() {
		let check_args = vec!["hc", "check"];
		let schema_args = vec!["hc", "schema"];

		let parsed = CliConfig::try_parse_from(check_args);
		assert!(parsed.is_err());
		assert_eq!(
			parsed.unwrap_err().kind(),
			clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
		);

		let parsed = CliConfig::try_parse_from(schema_args);
		assert!(parsed.is_err());
		assert_eq!(
			parsed.unwrap_err().kind(),
			clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
		);
	}

	fn get_check_cmd_from_cli(args: Vec<&str>) -> Result<CheckCommand> {
		let parsed = CliConfig::try_parse_from(args);
		assert!(parsed.is_ok());
		let command = parsed.unwrap().command;
		let Some(Commands::Check(chck_args)) = command else {
			unreachable!();
		};
		chck_args.command()
	}

	fn get_target_from_cmd(cmd: CheckCommand) -> String {
		match cmd {
			CheckCommand::Maven(args) => args.package,
			CheckCommand::Npm(args) => args.package,
			CheckCommand::Pypi(args) => args.package,
			CheckCommand::Repo(args) => args.source,
			CheckCommand::Sbom(args) => args.path,
		}
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
	fn test_deductive_check_github_purl() {
		let url = "https://github.com/mitre/hipcheck.git".to_string();
		let cmd = get_check_cmd_from_cli(vec!["hc", "check", "pkg:github/mitre/hipcheck"]);
		assert!(matches!(cmd, Ok(CheckCommand::Repo(..))));
		if let Ok(chk_cmd) = cmd {
			let target = get_target_from_cmd(chk_cmd);
			assert_eq!(target, url);
		}
	}

	#[test]
	fn test_deductive_check_maven_purl() {
		let url = "https://repo1.maven.org/maven2/org/apache/commons/commons-lang3/3.14.0/commons-lang3-3.14.0.pom".to_string();
		let cmd = get_check_cmd_from_cli(vec![
			"hc",
			"check",
			"pkg:maven/org.apache.commons/commons-lang3@3.14.0",
		]);
		assert!(matches!(cmd, Ok(CheckCommand::Maven(..))));
		if let Ok(chk_cmd) = cmd {
			let target = get_target_from_cmd(chk_cmd);
			assert_eq!(target, url);
		}
	}

	#[test]
	fn test_deductive_check_npm_purl() {
		let package = "@expressjs/express@4.19.2".to_string();
		let cmd =
			get_check_cmd_from_cli(vec!["hc", "check", "pkg:npm/%40expressjs/express@4.19.2"]);
		assert!(matches!(cmd, Ok(CheckCommand::Npm(..))));
		if let Ok(chk_cmd) = cmd {
			let target = get_target_from_cmd(chk_cmd);
			assert_eq!(target, package);
		}
	}

	#[test]
	fn test_deductive_check_pypi_purl() {
		let package = "django@5.0.7".to_string();
		let cmd = get_check_cmd_from_cli(vec!["hc", "check", "pkg:pypi/django@5.0.7"]);
		assert!(matches!(cmd, Ok(CheckCommand::Pypi(..))));
		if let Ok(chk_cmd) = cmd {
			let target = get_target_from_cmd(chk_cmd);
			assert_eq!(target, package);
		}
	}

	#[test]
	fn test_deductive_check_repo_vcs_https() {
		let url = "git+https://github.com/mitre/hipcheck.git".to_string();
		let cmd = get_check_cmd_from_cli(vec![
			"hc",
			"check",
			"git+https://github.com/mitre/hipcheck.git",
		]);
		assert!(matches!(cmd, Ok(CheckCommand::Repo(..))));
		if let Ok(chk_cmd) = cmd {
			let target = get_target_from_cmd(chk_cmd);
			assert_eq!(target, url);
		}
	}

	#[test]
	fn test_deductive_check_repo_vcs_ssh() {
		let url = "git+ssh://git@github.com/mitre/hipcheck.git".to_string();
		let cmd = get_check_cmd_from_cli(vec![
			"hc",
			"check",
			"git+ssh://git@github.com/mitre/hipcheck.git",
		]);
		assert!(matches!(cmd, Ok(CheckCommand::Repo(..))));
		if let Ok(chk_cmd) = cmd {
			let target = get_target_from_cmd(chk_cmd);
			assert_eq!(target, url);
		}
	}

	#[test]
	fn test_deductive_check_repo_filepath() {
		let path = "git+file:///home/me/projects/hipcheck".to_string();
		let cmd =
			get_check_cmd_from_cli(vec!["hc", "check", "git+file:///home/me/projects/hipcheck"]);
		assert!(matches!(cmd, Ok(CheckCommand::Repo(..))));
		if let Ok(chk_cmd) = cmd {
			let target = get_target_from_cmd(chk_cmd);
			assert_eq!(target, path);
		}
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

	#[test]
	fn test_check_with_incorrect_target_flag() {
		let cmd = get_check_cmd_from_cli(vec![
			"hc",
			"check",
			"-t",
			"npm",
			"https://github.com/mitre/hipcheck.git",
		]);
		assert!(matches!(cmd, Err(..)));
	}
}
