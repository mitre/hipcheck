// SPDX-License-Identifier: Apache-2.0

#[cfg(feature = "benchmarking")]
mod benchmarking;
mod cache;
mod cli;
mod config;
mod engine;
mod error;
mod exec;
mod init;
mod plugin;
mod policy;
mod policy_exprs;
mod report;
mod score;
mod session;
mod setup;
mod shell;
mod source;
mod target;
mod util;
mod version;

use crate::{
	cache::{plugin::HcPluginCache, repo::HcRepoCache},
	cli::Format,
	config::{normalized_unresolved_analysis_tree_from_policy, Config},
	engine::HcEngine,
	error::{Context as _, Error, Result},
	exec::ExecConfig,
	plugin::{try_set_arch, Plugin, PluginVersion, PluginWithConfig},
	policy::{config_to_policy, PolicyFile},
	report::report_builder::Report,
	session::ReadySession,
	session::Session,
	setup::write_config_binaries,
	shell::Shell,
};
use cli::{
	CacheOp, CachePluginArgs, CacheTargetArgs, CheckArgs, CliConfig, ConfigMode, FullCommands,
	PluginArgs, PluginOp, ReadyArgs, SchemaArgs, SchemaCommand, UpdateArgs,
};
use config::AnalysisTreeNode;
use core::fmt;
use indextree::{Arena, NodeId};
use ordered_float::NotNan;
use pathbuf::pathbuf;
use schemars::schema_for;
use shell::{color_choice::ColorChoice, spinner_phase::SpinnerPhase, verbosity::Verbosity};
use std::{
	env,
	fmt::{Display, Formatter},
	io::Write,
	ops::Not as _,
	path::{Path, PathBuf},
	process::{Command, ExitCode},
	result::Result as StdResult,
	time::Duration,
};
use target::{TargetSeed, ToTargetSeed};
use tokio::runtime::Runtime;
use tokio_stream::StreamExt;
use util::command::DependentProgram;
use util::fs::create_dir_all;
use which::which;

/// Entry point for Hipcheck.
fn main() -> ExitCode {
	init::init();

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
		Some(FullCommands::Check(mut args)) => return cmd_check(&mut args, &config),
		Some(FullCommands::Schema(args)) => cmd_schema(&args),
		Some(FullCommands::Setup) => return cmd_setup(&config),
		Some(FullCommands::Ready(args)) => return cmd_ready(&args, &config),
		Some(FullCommands::Update(args)) => cmd_update(&args),
		Some(FullCommands::CacheTarget(args)) => return cmd_cache_target(args, &config),
		Some(FullCommands::CachePlugin(args)) => return cmd_cache_plugin(args, &config),
		Some(FullCommands::Plugin(args)) => return cmd_plugin(args, &config),
		Some(FullCommands::PrintConfig) => cmd_print_config(config.config()),
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
fn cmd_check(args: &mut CheckArgs, config: &CliConfig) -> ExitCode {
	// Before we do any analysis, set the user-provided arch
	if let Some(arch) = &args.arch {
		if let Err(e) = try_set_arch(arch) {
			Shell::print_error(&e, Format::Human);
			return ExitCode::FAILURE;
		}
	}
	let target = match args.to_target_seed() {
		Ok(target) => target,
		Err(e) => {
			Shell::print_error(&e, Format::Human);
			return ExitCode::FAILURE;
		}
	};

	let config_mode = match config.config_mode() {
		Ok(config_mode) => config_mode,
		Err(e) => {
			Shell::print_error(&e, Format::Human);
			return ExitCode::FAILURE;
		}
	};

	log::info!("Using configuration source: {}", config_mode);

	// Panic: Safe to unwrap as Runtime::new() always returns as Ok
	let runtime = Runtime::new().unwrap();

	// TODO: In future work, we will not be looping through a collection of completed reports and emiting them all at once
	match run(
		target,
		config_mode,
		config.cache().map(ToOwned::to_owned),
		config.exec().map(ToOwned::to_owned),
		config.format(),
	) {
		Ok(mut reports) => {
			while let Some(report) = runtime.block_on(reports.next()) {
				match report {
					Ok(report) => {
						if let Err(err) = Shell::print_report(report, config.format()) {
							Shell::print_error(&err, config.format());
							return ExitCode::FAILURE;
						}
					}
					// Something went wrong with analyzing a specific dependency
					Err(e) => {
						Shell::print_error(&e, config.format());
						return ExitCode::FAILURE;
					}
				}
			}

			ExitCode::SUCCESS
		}
		// Something went wrong during setup or plugin initialization
		Err(e) => {
			Shell::print_error(&e, config.format());
			return ExitCode::FAILURE;
		}
	};

	ExitCode::SUCCESS
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
	let policy = if let Some(p) = config.policy() {
		PolicyFile::load_from(p)
		.context("Failed to load policy. Plase make sure the policy file is in the provided location and is formatted correctly.")?
	} else if let Some(c) = config.config() {
		let config = Config::load_from(c)
		.context("Failed to load configuration. If you have not yet done so on this system, try running `hc setup`. Otherwise, please make sure the config files are in the config directory.")?;
		config_to_policy(config, c)?
	} else {
		return Err(hc_error!("No policy file or (deprecated) config file found. Please provide a policy file before running Hipcheck."));
	};

	// Get the weight tree and print it.
	let weight_tree = normalized_unresolved_analysis_tree_from_policy(&policy)?;

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
		fn convert_tree(
			&mut self,
			old_root: NodeId,
			old_arena: &Arena<AnalysisTreeNode>,
		) -> NodeId {
			// Get a reference to the old node.
			let old_node = old_arena
				.get(old_root)
				.expect("root is present in old arena");

			// Format the new node depending on whether there are any children.
			let new_node = if old_root.children(old_arena).count() == 0 {
				// If no children, include the weight product.
				let weight_product = old_root
					.ancestors(old_arena)
					.map(|ancestor| old_arena.get(ancestor).unwrap().get().get_weight())
					.product::<NotNan<f64>>();

				// Format as percentage.
				PrintNode(format!(
					"{}: {:.2}%",
					old_node.get().get_print_label(),
					weight_product * 100f64
				))
			} else {
				PrintNode(old_node.get().get_print_label().clone())
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

	let mut print_tree = ConvertTree(Arena::with_capacity(weight_tree.tree.capacity()));
	let print_root = print_tree.convert_tree(weight_tree.root, &weight_tree.tree);

	// Print the output using indextree's debug pretty printer.
	let output = print_root.debug_pretty_print(&print_tree.0);
	shell::macros::println!("{output:?}");

	Ok(())
}

fn cmd_setup(config: &CliConfig) -> ExitCode {
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

	// Write config file binaries to target directory
	if let Err(e) = write_config_binaries(tgt_conf_path) {
		Shell::print_error(
			&hc_error!("failed to write config binaries to config dir {}", e),
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

	println!("Run `hc help` to get started");

	ExitCode::SUCCESS
}

#[derive(Debug)]
struct ReadyChecks {
	hipcheck_version_check: StdResult<String, VersionCheckError>,
	git_version_check: StdResult<String, VersionCheckError>,
	npm_version_check: StdResult<String, VersionCheckError>,
	cache_path_check: StdResult<PathBuf, PathCheckError>,
	policy_path_check: StdResult<PathBuf, PathCheckError>,
	plugin_check: StdResult<(), Error>,
}

impl ReadyChecks {
	/// Check if Hipcheck is ready to run.
	fn is_ready(&self) -> bool {
		self.hipcheck_version_check.is_ok()
			&& self.git_version_check.is_ok()
			&& self.npm_version_check.is_ok()
			&& self.cache_path_check.is_ok()
			&& self.policy_path_check.is_ok()
			&& self.plugin_check.is_ok()
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
	PolicyNotFound,
}

impl Display for PathCheckError {
	fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
		match self {
			PathCheckError::PathNotFound => write!(f, "Path not found"),
			PathCheckError::PolicyNotFound => write!(f, "Policy file not found. Specify the location of a policy file using the --policy flag.")
		}
	}
}

fn check_hipcheck_version() -> StdResult<String, VersionCheckError> {
	let pkg_name = env!("CARGO_PKG_NAME", "can't find Hipcheck package name");
	let version = env!("CARGO_PKG_VERSION", "can't find Hipcheck package version");
	let version = version::parse_hc_version(version).map_err(|_| VersionCheckError {
		cmd_name: "hc",
		kind: VersionCheckErrorKind::CmdNotFound,
	})?;
	Ok(format!("{} {}", pkg_name, version))
}

fn check_git_version() -> StdResult<String, VersionCheckError> {
	let version = util::git::get_git_version().map_err(|_| VersionCheckError {
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
	let version = util::npm::get_npm_version()
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

fn check_cache_path(config: &CliConfig) -> StdResult<PathBuf, PathCheckError> {
	let path = config.cache().ok_or(PathCheckError::PathNotFound)?;

	// Try to create the cache directory if it doesn't exist.
	if path.exists().not() {
		create_dir_all(path).map_err(|_| PathCheckError::PathNotFound)?;
	}

	Ok(path.to_owned())
}

fn check_policy_path(config: &CliConfig) -> StdResult<PathBuf, PathCheckError> {
	let path = config.policy().ok_or(PathCheckError::PolicyNotFound)?;

	if path.exists().not() {
		return Err(PathCheckError::PolicyNotFound);
	}

	Ok(path.to_owned())
}

fn check_plugins(config: &CliConfig) -> StdResult<(), Error> {
	let config_mode = config.config_mode()?;

	log::info!("Using configuration source: {}", config_mode);

	ReadySession::new(
		config_mode,
		config.cache().map(ToOwned::to_owned),
		config.exec().map(ToOwned::to_owned),
		config.format(),
	)
	.map(|_| ())
}

fn cmd_plugin(args: PluginArgs, config: &CliConfig) -> ExitCode {
	use crate::engine::{async_query, HcEngineImpl, PluginCore};
	use std::sync::Arc;
	use tokio::task::JoinSet;

	let version = PluginVersion::new("0.0.0").unwrap();
	let working_dir = PathBuf::from("./target/debug");

	let entrypoint1 = pathbuf!["dummy_rand_data"];
	let entrypoint2 = pathbuf!["dummy_sha256"];
	let plugin1 = Plugin {
		name: "dummy/rand_data".to_owned(),
		version: version.clone(),
		working_dir: working_dir.clone(),
		entrypoint: entrypoint1.display().to_string(),
	};
	let plugin2 = Plugin {
		name: "dummy/sha256".to_owned(),
		version: version.clone(),
		working_dir: working_dir.clone(),
		entrypoint: entrypoint2.display().to_string(),
	};
	let res_exec_config = if let Some(p) = config.exec() {
		ExecConfig::from_file(p)
			.context("Failed to load the provided exec config. Please make sure the exec config file is in the provided location and is formatted correctly.")
	} else {
		ExecConfig::find_file()
			.context("Failed to locate the exec config. Please make sure the exec config file exists somewhere in this directory or one of its parents as '.hipcheck/Exec.kdl'.")
	};

	let exec_config = match res_exec_config {
		Ok(config) => config,
		Err(e) => {
			Shell::print_error(
				&hc_error!("Failed to resolve the exec config {}", e),
				Format::Human,
			);
			return ExitCode::FAILURE;
		}
	};

	let plugin_executor = match ExecConfig::get_plugin_executor(&exec_config) {
		Ok(e) => e,
		Err(e) => {
			Shell::print_error(
				&hc_error!("Failed to resolve the Plugin Executor {}", e),
				Format::Human,
			);
			return ExitCode::FAILURE;
		}
	};

	let engine = match HcEngineImpl::new(
		plugin_executor,
		vec![
			PluginWithConfig(plugin1, serde_json::json!(null)),
			PluginWithConfig(plugin2, serde_json::json!(null)),
		],
	) {
		Ok(e) => e,
		Err(e) => {
			Shell::print_error(&hc_error!("Failed to create engine {}", e), Format::Human);
			return ExitCode::FAILURE;
		}
	};
	if args.asynch {
		// @Note - how to initiate multiple queries with async calls
		let core = engine.core();
		let handle = HcEngineImpl::runtime();
		handle.block_on(async move {
			let mut futs = JoinSet::new();
			for i in 1..10 {
				let arc_core = Arc::clone(&core);
				println!("Spawning");
				futs.spawn(async_query(
					arc_core,
					"dummy".to_owned(),
					"rand_data".to_owned(),
					"rand_data".to_owned(),
					serde_json::json!(i),
				));
			}
			while let Some(res) = futs.join_next().await {
				println!("res: {res:?}");
			}
		});
	} else {
		let res = engine.query(
			"dummy".to_owned(),
			"rand_data".to_owned(),
			"rand_data".to_owned(),
			serde_json::json!(1),
		);
		println!("res: {res:?}");
		// @Note - how to initiate multiple queries with sync calls
		// Currently does not work, compiler complains need Sync impl
		// use std::thread;
		// let conc: Vec<thread::JoinHandle<()>> = vec![];
		// for i in 0..10 {
		// 	let snapshot = engine.snapshot();
		// 	let fut = thread::spawn(|| {
		// 		let res = match snapshot.query(
		// 			"MITRE".to_owned(),
		// 			"rand_data".to_owned(),
		// 			"rand_data".to_owned(),
		// 			serde_json::json!(i),
		// 		) {
		// 			Ok(r) => r,
		// 			Err(e) => {
		// 				println!("{i}: Query failed: {e}");
		// 				return;
		// 			}
		// 		};
		// 		println!("{i}: Result: {res}");
		// 	});
		// 	conc.push(fut);
		// }
		// while let Some(x) = conc.pop() {
		// 	x.join().unwrap();
		// }
	}
	ExitCode::SUCCESS
}

fn cmd_ready(args: &ReadyArgs, config: &CliConfig) -> ExitCode {
	// Before we start plugins, set the user-provided arch
	if let Some(arch) = &args.arch {
		if let Err(e) = try_set_arch(arch) {
			Shell::print_error(&e, Format::Human);
			return ExitCode::FAILURE;
		}
	}
	let ready = ReadyChecks {
		hipcheck_version_check: check_hipcheck_version(),
		git_version_check: check_git_version(),
		npm_version_check: check_npm_version(),
		cache_path_check: check_cache_path(config),
		policy_path_check: check_policy_path(config),
		plugin_check: check_plugins(config),
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

	match &ready.policy_path_check {
		Ok(path) => println!("{:<17} {}", "Policy Path:", path.display()),
		Err(e) => println!("{:<17} {}", "Policy Path:", e),
	}

	match &ready.plugin_check {
		Ok(_) => println!("{:<17} All started successfully", "Plugins:"),
		Err(e) => println!(
			"{:<17} Some plugins did not start successfully\n\n{}",
			"Plugins:", e
		),
	}

	if ready.is_ready() {
		println!("Hipcheck is ready to run!");
		ExitCode::SUCCESS
	} else {
		println!("Hipcheck is NOT ready to run");
		ExitCode::FAILURE
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

fn cmd_cache_target(args: CacheTargetArgs, config: &CliConfig) -> ExitCode {
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
	let mut cache = HcRepoCache::new(path);
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

fn cmd_cache_plugin(args: CachePluginArgs, config: &CliConfig) -> ExitCode {
	let Some(path) = config.cache() else {
		println!("cache path must be defined by cmdline arg or $HC_CACHE env var");
		return ExitCode::FAILURE;
	};
	let op: PluginOp = match args.try_into() {
		Ok(o) => o,
		Err(e) => {
			println!("{e}");
			return ExitCode::FAILURE;
		}
	};
	let mut cache = HcPluginCache::new(path);
	let res = match op {
		PluginOp::List {
			scope,
			name,
			publisher,
			version,
		} => cache.list(scope, name, publisher, version),
		PluginOp::Delete {
			scope,
			name,
			publisher,
			version,
			force,
		} => cache.delete(scope, name, publisher, version, force),
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

// Constants for exiting with error codes.
/// Indicates the program failed.
const EXIT_FAILURE: i32 = 1;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum CheckKind {
	Npm,
	Pypi,
}

impl CheckKind {
	/// Get the name of the check.
	const fn name(&self) -> &'static str {
		match self {
			CheckKind::Npm => "npm",
			CheckKind::Pypi => "pypi",
		}
	}
}

// This is for testing purposes.
/// Now that we're fully-initialized, run Hipcheck's analyses.
#[allow(clippy::too_many_arguments)]
fn run(
	seed: TargetSeed,
	config_mode: ConfigMode,
	home_dir: Option<PathBuf>,
	exec_path: Option<PathBuf>,
	format: Format,
) -> Result<impl StreamExt<Item = Result<Report>>> {
	// Initialize a base session that will be cloned for each Target.
	let base_session = Session::new(config_mode, home_dir, exec_path, format)?;

	// Print the prelude
	Shell::print_prelude(seed.to_string());

	// Silence progress bars (if not already silenced) going forward if there are multiple targets
	if seed.is_multi_target() {
		Shell::set_verbosity(Verbosity::Quiet);
	}

	// Resolve the target from the target seed
	// TODO: Once we are analyzing targets in parallel, we will not be loading the entire list of Targets at once
	let targets = session::load_target(&seed, base_session.home())?;

	// Run analyses against a repo and score the results (score calls analyses that call metrics).
	let phase = SpinnerPhase::start("analyzing and scoring results");

	// Enable steady ticking on the spinner, since we currently don't increment it manually.
	phase.enable_steady_tick(Duration::from_millis(250));

	// Run and score plugins, then generate the final collection of reports
	// TODO: Once we are analyzing targets in parallel, we will be emitting scored reports as soon as they are build, instead of collecting them
	let mut reports = Vec::new();
	for target in targets {
		// Clone a new session for each target
		// TODO: Clone a fixed number of sessions, optionally specified by the user, and reuse them as they become free
		let mut session = base_session.clone();
		// Run plugins against the target, generate scores, and build a report
		// Clear the Salsa db and reset the start time, in case we are re-using an existing session
		let report = session.run(&phase, &target, true);
		// Add report to report collection
		reports.push(report);
	}

	Ok(tokio_stream::iter(reports))
}
