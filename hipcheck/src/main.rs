// SPDX-License-Identifier: Apache-2.0

#[allow(unused)]
mod analysis;
#[cfg(feature = "benchmarking")]
mod benchmarking;
mod cache;
mod cli;
mod command_util;
mod config;
mod context;
mod data;
mod error;
mod git2_log_shim;
mod git2_rustls_transport;
mod log_bridge;
mod metric;
#[allow(unused)]
mod plugin;
mod report;
mod session;
mod setup;
mod shell;
mod source;
mod target;
mod util;
mod version;

pub mod hipcheck {
	include!(concat!(env!("OUT_DIR"), "/hipcheck.rs"));
}

use crate::analysis::report_builder::build_report;
use crate::analysis::report_builder::AnyReport;
use crate::analysis::report_builder::Format;
use crate::analysis::report_builder::Report;
use crate::analysis::score::score_results;
use crate::cache::HcCache;
use crate::context::Context as _;
use crate::error::Error;
use crate::error::Result;
use crate::plugin::{HcPluginCore, Plugin, PluginExecutor, PluginWithConfig};
use crate::session::session::Session;
use crate::setup::{resolve_and_transform_source, SourceType};
use crate::shell::verbosity::Verbosity;
use crate::shell::Shell;
use crate::util::iter::TryAny;
use crate::util::iter::TryFilter;
use cli::CacheArgs;
use cli::CacheOp;
use cli::CheckArgs;
use cli::CliConfig;
use cli::FullCommands;
use cli::SchemaArgs;
use cli::SchemaCommand;
use cli::SetupArgs;
use cli::UpdateArgs;
use command_util::DependentProgram;
use config::WeightTreeNode;
use config::WeightTreeProvider;
use core::fmt;
use env_logger::Env;
use indextree::Arena;
use indextree::NodeId;
use ordered_float::NotNan;
use pathbuf::pathbuf;
use rustls::crypto::ring;
use rustls::crypto::CryptoProvider;
use schemars::schema_for;
use shell::color_choice::ColorChoice;
use shell::spinner_phase::SpinnerPhase;
use std::env;
use std::fmt::Display;
use std::fmt::Formatter;
use std::io::Write;
use std::ops::Not as _;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::ExitCode;
use std::result::Result as StdResult;
use std::time::Duration;
use target::{RemoteGitRepo, TargetSeed, TargetSeedKind, ToTargetSeed};
use util::fs::create_dir_all;
use which::which;

fn init_logging() -> std::result::Result<(), log::SetLoggerError> {
	let env = Env::new().filter("HC_LOG").write_style("HC_LOG_STYLE");

	let logger = env_logger::Builder::from_env(env).build();

	log_bridge::LogWrapper(logger).try_init()
}

/// Entry point for Hipcheck.
fn main() -> ExitCode {
	// Initialize the global shell with normal verbosity by default.
	Shell::init(Verbosity::Normal);
	// Initialize logging.
	// This must be done after shell initialization.
	// Panic if this fails.
	init_logging().unwrap();

	// Tell the git2 crate to pass its tracing messages to the log crate.
	git2_log_shim::git2_set_trace_log_shim();

	// Make libgit2 use a rustls + ureq based transport for executing the git protocol over http(s).
	// I would normally just let libgit2 use its own implementation but have seen that this rustls/ureq transport is
	// 2-3 times faster on my machine -- enough of a performance bump to warrant using this.
	git2_rustls_transport::register();

	// Install a process-wide default crypto provider.
	CryptoProvider::install_default(ring::default_provider())
		.expect("installed process-wide default crypto provider");

	if cfg!(feature = "print-timings") {
		Shell::eprintln("[TIMINGS]: Timing information will be printed.");
	}

	// Start tracking the timing for `main` after logging is initiated.
	#[cfg(feature = "print-timings")]
	let _0 = benchmarking::print_scope_time!("main");

	let config = CliConfig::load();

	// Set the global verbosity.
	Shell::set_verbosity(config.verbosity());

	// Set whether to use colors.
	match config.color() {
		ColorChoice::Always => Shell::set_colors_enabled(true),
		ColorChoice::Never => Shell::set_colors_enabled(false),
		ColorChoice::Auto => {}
	}

	match config.subcommand() {
		Some(FullCommands::Check(args)) => return cmd_check(&args, &config),
		Some(FullCommands::Schema(args)) => cmd_schema(&args),
		Some(FullCommands::Setup(args)) => return cmd_setup(&args, &config),
		Some(FullCommands::Ready) => cmd_ready(&config),
		Some(FullCommands::Update(args)) => cmd_update(&args),
		Some(FullCommands::Cache(args)) => return cmd_cache(args, &config),
		Some(FullCommands::Plugin) => cmd_plugin(),
		Some(FullCommands::PrintConfig) => cmd_print_config(config.config()),
		Some(FullCommands::PrintData) => cmd_print_data(config.data()),
		Some(FullCommands::PrintCache) => cmd_print_home(config.cache()),
		Some(FullCommands::Scoring) => {
			return cmd_print_weights(&config)
				.map(|_| ExitCode::SUCCESS)
				.unwrap_or_else(|err| {
					Shell::print_error(&err, Format::Human);
					ExitCode::FAILURE
				});
		}

		None => Shell::print_error(&hc_error!("missing subcommand"), Format::Human),
	};

	// If we didn't early return, return success.
	ExitCode::SUCCESS
}

/// Run the `check` command.
fn cmd_check(args: &CheckArgs, config: &CliConfig) -> ExitCode {
	let target = match args.to_target_seed() {
		Ok(target) => target,
		Err(e) => {
			Shell::print_error(&e, Format::Human);
			return ExitCode::FAILURE;
		}
	};

	let raw_version = env!("CARGO_PKG_VERSION", "can't find Hipcheck package version");

	let report = run(
		target,
		config.config().map(ToOwned::to_owned),
		config.data().map(ToOwned::to_owned),
		config.cache().map(ToOwned::to_owned),
		config.format(),
		raw_version,
	);

	match report {
		Ok(AnyReport::Report(report)) => Shell::print_report(report, config.format())
			.map(|()| ExitCode::SUCCESS)
			.unwrap_or_else(|err| {
				Shell::print_error(&err, Format::Human);
				ExitCode::FAILURE
			}),
		Err(e) => {
			Shell::print_error(&e, config.format());
			ExitCode::FAILURE
		}
	}
}

/// Run the `schema` command.
fn cmd_schema(args: &SchemaArgs) {
	match args.command {
		SchemaCommand::Maven => print_maven_schema(),
		SchemaCommand::Npm => print_npm_schema(),
		SchemaCommand::Pypi => print_pypi_schema(),
		SchemaCommand::Repo => print_report_schema(),
	}
}

fn cmd_print_weights(config: &CliConfig) -> Result<()> {
	// Get the raw hipcheck version.
	let raw_version = env!("CARGO_PKG_VERSION", "can't find Hipcheck package version");

	// Silence the global shell while we're checking the dummy repo to prevent progress bars and
	// title messages from displaying while calculating the weight tree.
	let silence_guard = Shell::silence();

	// Create a dummy session to query the salsa database for a weight graph for printing.
	let session = Session::new(
		// Use the hipcheck repo as a dummy url until checking is de-coupled from `Session`.
		&TargetSeed {
			kind: TargetSeedKind::RemoteRepo(RemoteGitRepo {
				url: url::Url::parse("https://github.com/mitre/hipcheck.git").unwrap(),
				known_remote: None,
			}),
			refspec: Some("HEAD".to_owned()),
		},
		config.config().map(ToOwned::to_owned),
		config.data().map(ToOwned::to_owned),
		config.cache().map(ToOwned::to_owned),
		config.format(),
		raw_version,
	)?;

	// Get the weight tree and print it.
	let weight_tree = session.normalized_weight_tree()?;

	// Create a special wrapper to override `Debug` so that we can use indextree's \
	// debug pretty print function instead of writing our own.
	struct PrintNode(String);

	impl std::fmt::Debug for PrintNode {
		fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
			f.write_str(self.0.as_ref())
		}
	}

	// Struct to help us convert the tree to PrintNodes.
	// This has to be a struct not a closure because we use a recursive function to convert the tree.
	struct ConvertTree(Arena<PrintNode>);

	impl ConvertTree {
		fn convert_tree(&mut self, old_root: NodeId, old_arena: &Arena<WeightTreeNode>) -> NodeId {
			// Get a reference to the old node.
			let old_node = old_arena
				.get(old_root)
				.expect("root is present in old arena");

			// Format the new node depending on whether there are any children.
			let new_node = if old_root.children(old_arena).count() == 0 {
				// If no children, include the weight product.
				let weight_product = old_root
					.ancestors(old_arena)
					.map(|ancestor| old_arena.get(ancestor).unwrap().get().weight)
					.product::<NotNan<f64>>();

				// Format as percentage.
				PrintNode(format!(
					"{}: {:.2}%",
					old_node.get().label,
					weight_product * 100f64
				))
			} else {
				PrintNode(old_node.get().label.clone())
			};

			// Add the new node to the new arena.
			let new_root = self.0.new_node(new_node);

			// Recursively add all children.
			for child in old_root.children(old_arena) {
				// Convert the child node and its subnodes.
				let new_child_id = self.convert_tree(child, old_arena);

				// Attach the child node and its tree as a child of this node.
				new_root.append(new_child_id, &mut self.0);
			}

			// Return the new root's ID.
			new_root
		}
	}

	// Drop the silence guard to make the shell produce output again.
	drop(silence_guard);

	let mut print_tree = ConvertTree(Arena::with_capacity(weight_tree.tree.capacity()));
	let print_root = print_tree.convert_tree(weight_tree.root, &weight_tree.tree);

	// Print the output using indextree's debug pretty printer.
	let output = print_root.debug_pretty_print(&print_tree.0);
	shell::macros::println!("{output:?}");

	Ok(())
}

/// Copy individual files in dir instead of entire dir, to avoid users accidentally
/// overwriting important dirs such as /usr/bin/
fn copy_dir_contents<P: AsRef<Path>, Q: AsRef<Path>>(from: P, to: Q) -> Result<()> {
	fn inner(from: &Path, to: &Path) -> Result<()> {
		let src = from.to_path_buf();
		if !src.is_dir() {
			return Err(hc_error!("source path must be a directory"));
		}
		let dst: PathBuf = to.to_path_buf();
		if !dst.is_dir() {
			return Err(hc_error!("target path must be a directory"));
		}

		for entry in walkdir::WalkDir::new(&src) {
			let src_f_path = entry?.path().to_path_buf();
			if src_f_path == src {
				continue;
			}
			let mut dst_f_path = dst.clone();
			dst_f_path.push(
				src_f_path
					.file_name()
					.ok_or(hc_error!("src dir entry without file name"))?,
			);
			// This is ok for now because we only copy files, no dirs
			std::fs::copy(src_f_path, dst_f_path)?;
		}
		Ok(())
	}
	inner(from.as_ref(), to.as_ref())
}

fn cmd_setup(args: &SetupArgs, config: &CliConfig) -> ExitCode {
	// Find or download a Hipcheck bundle source and decompress
	let source = match resolve_and_transform_source(args) {
		Err(e) => {
			Shell::print_error(&e, Format::Human);
			return ExitCode::FAILURE;
		}
		Ok(x) => x,
	};

	// Derive the config/scripts paths from the source path
	let (src_conf_path, src_data_path) = match &source.path {
		SourceType::Dir(p) => (
			pathbuf![p.as_path(), "config"],
			pathbuf![p.as_path(), "scripts"],
		),
		_ => {
			Shell::print_error(
				&hc_error!("expected source to be a directory"),
				Format::Human,
			);
			source.cleanup();
			return ExitCode::FAILURE;
		}
	};

	// Make config dir if not exist
	let Some(tgt_conf_path) = config.config() else {
		Shell::print_error(&hc_error!("target config dir not specified"), Format::Human);
		return ExitCode::FAILURE;
	};

	if !tgt_conf_path.exists() && create_dir_all(tgt_conf_path).is_err() {
		Shell::print_error(
			&hc_error!("failed to create missing target config dir"),
			Format::Human,
		);
	}

	let Ok(abs_conf_path) = tgt_conf_path.canonicalize() else {
		Shell::print_error(
			&hc_error!("failed to canonicalize HC_CONFIG path"),
			Format::Human,
		);
		return ExitCode::FAILURE;
	};

	// Make data dir if not exist
	let Some(tgt_data_path) = config.data() else {
		Shell::print_error(&hc_error!("target data dir not specified"), Format::Human);
		return ExitCode::FAILURE;
	};
	if !tgt_data_path.exists() && create_dir_all(tgt_data_path).is_err() {
		Shell::print_error(
			&hc_error!("failed to create missing target data dir"),
			Format::Human,
		);
	}
	let Ok(abs_data_path) = tgt_data_path.canonicalize() else {
		Shell::print_error(
			&hc_error!("failed to canonicalize HC_DATA path"),
			Format::Human,
		);
		return ExitCode::FAILURE;
	};

	// Copy local config/data dirs to target locations
	if let Err(e) = copy_dir_contents(src_conf_path, &abs_conf_path) {
		Shell::print_error(
			&hc_error!("failed to copy config dir contents: {}", e),
			Format::Human,
		);
		return ExitCode::FAILURE;
	}
	if let Err(e) = copy_dir_contents(src_data_path, &abs_data_path) {
		Shell::print_error(
			&hc_error!("failed to copy data dir contents: {}", e),
			Format::Human,
		);
		return ExitCode::FAILURE;
	}

	println!("Hipcheck setup completed successfully.");

	// Recommend exportation of HC_CONFIG/HC_DATA env vars if applicable
	let shell_profile = match std::env::var("SHELL").as_ref().map(String::as_str) {
		Ok("/bin/zsh") | Ok("/usr/bin/zsh") => ".zshrc",
		Ok("/bin/bash") | Ok("/usr/bin/bash") => ".bash_profile",
		_ => ".profile",
	};

	println!(
		"Manually add the following to your '$HOME/{}' (or similar) if you haven't already",
		shell_profile
	);
	println!("\texport HC_CONFIG={:?}", abs_conf_path);
	println!("\texport HC_DATA={:?}", abs_data_path);

	println!("Run `hc help` to get started");

	source.cleanup();

	ExitCode::SUCCESS
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

fn cmd_plugin() {
	use tokio::runtime::Runtime;
	let tgt_dir = "./target/debug";
	let entrypoint = pathbuf![tgt_dir, "dummy_rand_data"];
	let plugin = Plugin {
		name: "rand_data".to_owned(),
		entrypoint: entrypoint.display().to_string(),
	};
	let plugin_executor = PluginExecutor::new(
		/* max_spawn_attempts */ 3,
		/* max_conn_attempts */ 5,
		/* port_range */ 40000..u16::MAX,
		/* backoff_interval_micros */ 1000,
		/* jitter_percent */ 10,
	)
	.unwrap();
	let rt = Runtime::new().unwrap();
	rt.block_on(async move {
		println!("Started executor");
		let mut core = match HcPluginCore::new(
			plugin_executor,
			vec![PluginWithConfig(plugin, serde_json::json!(null))],
		)
		.await
		{
			Ok(c) => c,
			Err(e) => {
				println!("{e}");
				return;
			}
		};
		match core.run().await {
			Ok(_) => {
				println!("HcCore run completed");
			}
			Err(e) => {
				println!("HcCore run failed with '{e}'");
			}
		};
	});
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

/// Run the Hipcheck self-updater to update to the latest release version.
/// If the updater is not found, returns an error
fn cmd_update(args: &UpdateArgs) {
	let command_name;
	// Because of a bug in cargo-dist's updater, it is possible for the updater to be installed as "hipcheck-update" instead of "hc-update"
	if which("hc-update").is_ok() {
		command_name = "hc-update";
	} else if which("hipcheck-update").is_ok() {
		command_name = "hipcheck-update";
	} else {
		// If neither possible updater command us found, print this error
		Shell::print_error(&hc_error!("Updater tool not found. Did you install Hipcheck with the official release script (which will install the updater tool)? If you installed Hipcheck from source, you must update Hipcheck by installing a new version from source manually."), Format::Human);
		return;
	}

	// Create the updater command, with optional arguments
	let mut hc_command = updater_command(command_name, args);
	match hc_command.output() {
		// Panic: Safe to unwrap because if the updater command runs, it will always produce some output to stderr
		Ok(output) => std::io::stdout().write_all(&output.stderr).unwrap(),
		Err(..) => Shell::print_error(&hc_error!("Updater command failed to run. You may need to re-install Hipcheck with the official release script."), Format::Human),
	}
}

/// Creates an updater command, including any optional arguments
fn updater_command(command_name: &str, args: &UpdateArgs) -> Command {
	let mut command = Command::new(command_name);

	// If both the --tag and --version arguments are passed to the updater, it will exit with an error message instead of updating
	if let Some(tag) = &args.tag {
		command.args(["--tag", tag]);
	}

	if let Some(version) = &args.version {
		command.args(["--version", version]);
	}

	if args.prerelease {
		command.arg("--prerelease");
	}

	command
}

fn cmd_cache(args: CacheArgs, config: &CliConfig) -> ExitCode {
	let Some(path) = config.cache() else {
		println!("cache path must be defined by cmdline arg or $HC_CACHE env var");
		return ExitCode::FAILURE;
	};
	let op: CacheOp = match args.try_into() {
		Ok(o) => o,
		Err(e) => {
			println!("{e}");
			return ExitCode::FAILURE;
		}
	};
	let mut cache = HcCache::new(path);
	let res = match op {
		CacheOp::List { scope, filter } => cache.list(scope, filter),
		CacheOp::Delete {
			scope,
			filter,
			force,
		} => cache.delete(scope, filter, force),
	};
	drop(cache);
	if let Err(e) = res {
		println!("{e}");
		ExitCode::FAILURE
	} else {
		ExitCode::SUCCESS
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
			Shell::print_error(&err, Format::Human);
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
			Shell::print_error(&err, Format::Human);
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
			Shell::print_error(&err, Format::Human);
		}
	}
}

/// Print the JSON schema of the report.
fn print_report_schema() {
	let schema = schema_for!(Report);
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
	#[allow(dead_code)]
	Repo,
	Maven,
	Npm,
	Pypi,
	#[allow(dead_code)]
	Spdx,
}

impl CheckKind {
	/// Get the name of the check.
	const fn name(&self) -> &'static str {
		match self {
			CheckKind::Repo => "repo",
			CheckKind::Maven => "maven",
			CheckKind::Npm => "npm",
			CheckKind::Pypi => "pypi",
			CheckKind::Spdx => "spdx",
		}
	}
}

// This is for testing purposes.
/// Now that we're fully-initialized, run Hipcheck's analyses.
#[allow(clippy::too_many_arguments)]
#[doc(hidden)]
fn run(
	target: TargetSeed,
	config_path: Option<PathBuf>,
	data_path: Option<PathBuf>,
	home_dir: Option<PathBuf>,
	format: Format,
	raw_version: &str,
) -> Result<AnyReport> {
	// Initialize the session.
	let session = match Session::new(
		&target,
		config_path,
		data_path,
		home_dir,
		format,
		raw_version,
	) {
		Ok(session) => session,
		Err(err) => return Err(err),
	};

	// Run analyses against a repo and score the results (score calls analyses that call metrics).
	let phase = SpinnerPhase::start("analyzing and scoring results");

	// Enable steady ticking on the spinner, since we currently don't increment it manually.
	phase.enable_steady_tick(Duration::from_millis(250));

	let scoring = score_results(&phase, &session)?;
	phase.finish_successful();

	// Build the final report.
	let report = build_report(&session, &scoring).context("failed to build final report")?;

	Ok(AnyReport::Report(report))
}
