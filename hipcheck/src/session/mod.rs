// SPDX-License-Identifier: Apache-2.0

use crate::{
	cache::plugin::HcPluginCache,
	cli::{ConfigMode, Format},
	config::Config,
	engine::{start_plugins, HcEngine, HcEngineStorage, HcPluginCore},
	error::{Context as _, Error, Result},
	exec::ExecConfig,
	hc_error,
	policy::{config_to_policy, PolicyFile},
	shell::{spinner_phase::SpinnerPhase, Shell},
	target::{
		resolve::{TargetResolver, TargetResolverConfig},
		KnownRemote, Target, TargetSeed, TargetSeedKind,
	},
};
use chrono::prelude::*;
use std::{
	fmt,
	path::{Path, PathBuf},
	result::Result as StdResult,
	sync::Arc,
	time::Duration,
};

struct SessionBuilder {
	config_dir: Option<PathBuf>,
	cache_dir: Option<PathBuf>,
	policy_file_path: Option<PathBuf>,
	policy_file: Option<PolicyFile>,
	target: Option<Target>,
	exec_config: Option<ExecConfig>,
	format: Format,
	started_at: chrono::DateTime<Local>,
}

impl SessionBuilder {
	fn new(start_time: chrono::DateTime<Local>, format: Format) -> Self {
		Self {
			config_dir: None,
			cache_dir: None,
			policy_file_path: None,
			policy_file: None,
			target: None,
			exec_config: None,
			format,
			started_at: start_time,
		}
	}

	fn set_config_dir(&mut self, config_dir: Option<PathBuf>) -> &mut Self {
		self.config_dir = config_dir;
		self
	}

	fn set_cache_dir(&mut self, cache_dir: PathBuf) -> &mut Self {
		self.cache_dir = Some(cache_dir);
		self
	}

	fn set_policy(&mut self, policy_file: PolicyFile) -> &mut Self {
		self.policy_file = Some(policy_file);
		self
	}

	fn get_policy(&self) -> Option<&PolicyFile> {
		self.policy_file.as_ref()
	}

	fn set_policy_path(&mut self, policy_path: Option<PathBuf>) -> &mut Self {
		self.policy_file_path = policy_path;
		self
	}

	fn set_target(&mut self, target: Target) -> &mut Self {
		self.target = Some(target);
		self
	}

	fn set_exec_config(&mut self, exec_config: ExecConfig) -> &mut Self {
		self.exec_config = Some(exec_config);
		self
	}

	fn get_exec_config(&mut self) -> Option<&ExecConfig> {
		self.exec_config.as_ref()
	}

	fn build(self, core: Arc<HcPluginCore>) -> Result<Session> {
		let target = match self.target {
			Some(target) => target,
			None => return Err(hc_error!("Missing Target")),
		};

		let exec_config = match self.exec_config {
			Some(c) => c,
			None => return Err(hc_error!("Missing ExecConfig")),
		};

		let policy_file = match self.policy_file {
			Some(policy_file) => policy_file,
			None => return Err(hc_error!("Missing PolicyFile")),
		};

		let mut session = Session {
			storage: Default::default(),
			target,
			policy_file,
			exec_config,
			started_at: self.started_at,
			format: self.format,
		};
		// ensure core is set, needed by salsa
		session.set_core(core);
		Ok(session)
	}
}

/// Immutable configuration and base data for a run of Hipcheck.
#[salsa::database(HcEngineStorage)]
pub struct Session {
	/// Query storage (used by salsa)
	storage: salsa::Storage<Self>,
	/// target of the analysis
	target: Target,
	/// format to display results in to end-user
	format: Format,
	/// policy file used to configure session
	policy_file: PolicyFile,
	/// configuration for plugins
	exec_config: ExecConfig,
	/// when session started
	started_at: DateTime<Local>,
}

// Required by our query groups
impl salsa::Database for Session {}

// Cannot be derived because `salsa::Storage` does not implement it
impl fmt::Debug for Session {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("Session")
			.field("storage", &"salsa::Storage<Session>")
			.field("target", &self.target)
			.field("format", &self.format)
			.field("policy_file", &self.policy_file)
			.field("exec_config", &self.exec_config)
			.field("started_at", &self.started_at)
			.finish()
	}
}

impl Session {
	// Note that error handling in the constructor for `Session` is a little awkward.
	// This is because we want to be able to hand back the `Shell` passed in if setup
	// fails, so instead of using the `?` operator, we need to do the returning manually.
	//
	// You may think we could use `map_err` and the question mark operator to bundle
	// the shell with whatever error we have and hand them back, but unfortunately this
	// doesn't work. When you use `shell` in the `map_err` closure, you're moving it
	// unconditionally, even though `map_err`'s closure is only run in the case of
	// an error (in which case you're also returning early), the Rust compiler isn't
	// smart enough to figure that out. Maybe this will improve in the future, but for
	// now, we have to do it by hand.

	/// Construct a new `Session` which owns all the data needed in later phases.
	#[allow(clippy::too_many_arguments)]
	pub fn new(
		target: &TargetSeed,
		config_mode: ConfigMode,
		home_dir: Option<PathBuf>,
		exec_path: Option<PathBuf>,
		format: Format,
	) -> StdResult<Session, Error> {
		/*===================================================================
		 *  Setting up the session builder
		 *-----------------------------------------------------------------*/

		let mut session_builder = SessionBuilder::new(chrono::Local::now(), format);

		/*===================================================================
		 * Printing the prelude.
		 *-----------------------------------------------------------------*/

		Shell::print_prelude(target.to_string());

		/*===================================================================
		 *  Loading configuration.
		 *-----------------------------------------------------------------*/

		use ConfigMode::*;
		match config_mode {
			PreferPolicy { policy, config } => {
				let res = use_policy(policy, &mut session_builder);
				if let Some(err) = res.err() {
					log::error!("Failed to load default policy KDL file; trying legacy config TOML directory instead. Error: {:#?}", err);

					use_config(config, &mut session_builder)?;
				}
			}
			ForcePolicy { policy } => {
				use_policy(policy, &mut session_builder)?;
			}
			ForceConfig { config } => {
				use_config(config, &mut session_builder)?;
			}
		}

		// Force eval the risk policy expr - wouldn't be necessary if the PolicyFile parsed
		let _ = session_builder
			.get_policy()
			.ok_or_else(|| hc_error!("PolicyFile not set yet"))?
			.risk_policy()?;

		/*===================================================================
		 *  Load the Exec Configuration
		 *-----------------------------------------------------------------*/
		let exec = load_exec_config(exec_path.as_deref())?;
		session_builder.set_exec_config(exec);

		/*===================================================================
		 *  Resolving the Hipcheck home.
		 *-----------------------------------------------------------------*/

		let home = home_dir
			.as_deref()
			.map(ToOwned::to_owned)
			.ok_or_else(|| hc_error!("can't find cache directory"))?;
		session_builder.set_cache_dir(home.clone());
		let plugin_cache = HcPluginCache::new(&home);

		/*===================================================================
		 *  Resolving the source.
		 *-----------------------------------------------------------------*/
		let target = load_target(target, &home)?;
		session_builder.set_target(target);

		/*===================================================================
		 *  Plugin startup.
		 *-----------------------------------------------------------------*/

		// The fact that we set the policy above to be accessible through the salsa
		// infrastructure would suggest that plugin startup should also be done
		// through salsa. Our goal is to produce an HcPluginCore instance, which
		// has all the plugins up and running. However, HcPluginCore does not impl
		// equal, and the idea of memoizing/invalidating it does not make sense.
		// Thus, we will do the plugin startup here.

		let policy = session_builder
			.get_policy()
			.ok_or_else(|| hc_error!("PolicyFile not set"))?
			.clone();

		let exec_config = session_builder
			.get_exec_config()
			.ok_or_else(|| hc_error!("ExecConfig not set"))?
			.clone();

		let executor = ExecConfig::get_plugin_executor(&exec_config)?;

		let core = start_plugins(&policy, &plugin_cache, executor)?;
		session_builder.build(core)
	}

	/// target of this analysis
	pub fn target(&self) -> Target {
		self.target.clone()
	}

	pub fn policy_file(&self) -> &PolicyFile {
		&self.policy_file
	}

	/// git ref of the HEAD commit being analyzed
	pub fn head(&self) -> Arc<String> {
		Arc::new(self.target.local.git_ref.clone())
	}

	/// gets the owner if there is a known Github remote repository
	pub fn owner(&self) -> Option<Arc<String>> {
		// Gets the owner if there is a known GitHub remote repository
		let KnownRemote::GitHub { owner, repo: _ } =
			&self.target.remote.as_ref()?.known_remote.as_ref()?;
		Some(Arc::new(owner.clone()))
	}

	/// name of the repository being analyzed
	pub fn name(&self) -> Arc<String> {
		// In the future may want to augment Target/LocalGitRepo with a
		// "name" field. For now, treat the dir name of the repo as the name
		Arc::new(
			self.target
				.local
				.path
				.as_path()
				.file_name()
				.unwrap()
				.to_str()
				.unwrap()
				.to_owned(),
		)
	}

	/// When the Session started
	pub fn started_at(&self) -> DateTime<Local> {
		self.started_at
	}

	/// Format to use for outputing session results
	pub fn format(&self) -> Format {
		self.format
	}
}

fn use_config(config_path: PathBuf, session_builder: &mut SessionBuilder) -> Result<()> {
	let (policy, config_dir) = load_config_and_data(config_path)?;

	// Set config dir
	session_builder
		.set_config_dir(Some(config_dir))
		.set_policy(policy)
		.set_policy_path(None);
	Ok(())
}

fn use_policy(policy_path: PathBuf, session_builder: &mut SessionBuilder) -> Result<()> {
	let (policy, policy_path) = load_policy_and_data(policy_path)?;
	// No config dir
	session_builder
		.set_config_dir(None)
		.set_policy(policy)
		.set_policy_path(Some(policy_path));
	Ok(())
}

pub fn load_config_and_data(config_path: PathBuf) -> Result<(PolicyFile, PathBuf)> {
	// Start the phase.
	let phase = SpinnerPhase::start("Loading configuration and data files from config file. Note: The use of a config TOML file is deprecated. Please consider using a policy KDL file in the future.");
	// Increment the phase into the "running" stage.
	phase.inc();
	// Set the spinner phase to tick constantly, 10 times a second.
	phase.enable_steady_tick(Duration::from_millis(100));

	// Load the configuration file.
	let config = Config::load_from(&config_path)
		.context("Failed to load configuration. If you have not yet done so on this system, try running `hc setup`. Otherwise, please make sure the config files are in the config directory.")?;

	// Convert the Config struct to a PolicyFile struct
	let policy = config_to_policy(config, &config_path)?;

	phase.finish_successful();

	Ok((policy, config_path))
}

pub fn load_policy_and_data(policy_path: PathBuf) -> Result<(PolicyFile, PathBuf)> {
	// Start the phase.
	let phase = SpinnerPhase::start("loading policy and data files");
	// Increment the phase into the "running" stage.
	phase.inc();
	// Set the spinner phase to tick constantly, 10 times a second.
	phase.enable_steady_tick(Duration::from_millis(100));

	// Load the policy file.
	let policy = PolicyFile::load_from(&policy_path)
		.with_context(|| format!("Failed to load policy file at path {:?}. Please make sure the policy file is in the provided location and is formatted correctly.", policy_path))?;

	phase.finish_successful();

	Ok((policy, policy_path))
}

fn load_exec_config(exec_path: Option<&Path>) -> Result<ExecConfig> {
	// Start the phase
	let phase = SpinnerPhase::start("loading exec config");
	// Increment the phase into the "running" stage.
	phase.inc();
	// Set the spinner phase to tick constantly, 10 times a second.
	phase.enable_steady_tick(Duration::from_millis(100));

	// Resolve the path to the exec config file.
	let exec_config = match exec_path {
		Some(p) => {
			// Use the path provided
			if !p.exists() {
				return Err(hc_error!("Failed to load exec config. Please make sure the path set by the --exec flag exists."));
			}
			ExecConfig::from_file(p)
				.context("Failed to load the exec config. Please make sure the exec config file is in the provided location and is formatted correctly.")?
		}
		None => {
			// Search for file or load the default if not provided
			ExecConfig::find_file()
				.context("Failed to load the default config. Please ensure the Exec.kdl is in the current directory or in .hipcheck/Exec.kdl of a parent directory.")?
		}
	};

	phase.finish_successful();

	Ok(exec_config)
}

fn load_target(seed: &TargetSeed, home: &Path) -> Result<Target> {
	// Resolve the source specifier into an actual source.
	let phase_desc = match seed.kind {
		TargetSeedKind::LocalRepo(_) | TargetSeedKind::RemoteRepo(_) => {
			"resolving git repository target"
		}
		TargetSeedKind::Package(_) => "resolving package target",
		TargetSeedKind::Sbom(_) => "parsing SBOM document",
		TargetSeedKind::MavenPackage(_) => "resolving maven package target",
	};

	let phase = SpinnerPhase::start(phase_desc);
	// Set the phase to tick steadily 10 times a second.
	phase.enable_steady_tick(Duration::from_millis(100));
	let target = resolve_target(seed, &phase, home)?;
	phase.finish_successful();

	Ok(target)
}

/// Resolves the target specifier into an actual target.
fn resolve_target(seed: &TargetSeed, phase: &SpinnerPhase, home: &Path) -> Result<Target> {
	#[cfg(feature = "print-timings")]
	let _0 = crate::benchmarking::print_scope_time!("resolve_target");

	let conf = TargetResolverConfig {
		phase: Some(phase.clone()),
		cache: PathBuf::from(home),
	};
	TargetResolver::resolve(conf, seed.clone())
}
