// SPDX-License-Identifier: Apache-2.0

use crate::{
	cache::plugin::HcPluginCache,
	cli::{ConfigMode, Format},
	config::Config,
	engine::{start_plugins, HcPluginCore, PluginCore},
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
	core: Option<Arc<HcPluginCore>>,
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
			core: None,
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

	fn get_cache_dir(&self) -> Option<&PathBuf> {
		self.cache_dir.as_ref()
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

	fn set_core(&mut self, core: Arc<HcPluginCore>) -> &mut Self {
		self.core = Some(core);
		self
	}

	fn build(self) -> Result<Session> {
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

		let core = match self.core {
			Some(core) => core,
			None => return Err(hc_error!("Missing HcPluginCore")),
		};

		let session = Session {
			storage: Default::default(),
			target,
			policy_file,
			exec_config,
			started_at: self.started_at,
			format: self.format,
			core,
		};
		Ok(session)
	}

	fn build_ready(self) -> Result<ReadySession> {
		if self.target.is_some() {
			return Err(hc_error!("Target should not be set for ReadySession"));
		};

		let exec_config = match self.exec_config {
			Some(c) => c,
			None => return Err(hc_error!("Missing ExecConfig")),
		};

		let policy_file = match self.policy_file {
			Some(policy_file) => policy_file,
			None => return Err(hc_error!("Missing PolicyFile")),
		};

		let session = ReadySession {
			policy_file,
			exec_config,
			started_at: self.started_at,
			format: self.format,
		};

		Ok(session)
	}
}

/// Immutable configuration and base data for a run of Hipcheck.
#[salsa::db]
#[derive(Clone)]
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
	/// core handle to plugins
	core: Arc<HcPluginCore>,
}

// Required by our query groups
#[salsa::db]
impl salsa::Database for Session {
	fn salsa_event(&self, event: &dyn Fn() -> salsa::Event) {
		let event = event();
		log::debug!("{:?}", event);
	}
}

impl PluginCore for Session {
	fn core(&self) -> Arc<HcPluginCore> {
		self.core.clone()
	}
}

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
			.field("core", &self.core)
			.finish()
	}
}

impl Session {
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
		 * Printing the prelude.
		 *-----------------------------------------------------------------*/

		Shell::print_prelude(target.to_string());

		let mut session_builder = setup_base_session(config_mode, home_dir, exec_path, format)?;

		/*===================================================================
		 *  Resolving the source.
		 *-----------------------------------------------------------------*/

		let home = session_builder.get_cache_dir().ok_or_else(|| {
			hc_error!("Internal error: SessionBuilder doesn't have cache_dir set")
		})?;
		let target = load_target(target, home)?;
		session_builder.set_target(target);

		session_builder.build()
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

/// A subset of Session that's used for the `hc ready` command.
/// It doesn't include a target or support for queries.
#[allow(unused)]
pub struct ReadySession {
	/// format to display results in to end-user
	format: Format,
	/// policy file used to configure session
	policy_file: PolicyFile,
	/// configuration for plugins
	exec_config: ExecConfig,
	/// when session started
	started_at: DateTime<Local>,
}

impl ReadySession {
	/// Construct a new `ReadySession` which owns all the data needed in later phases.
	pub fn new(
		config_mode: ConfigMode,
		home_dir: Option<PathBuf>,
		exec_path: Option<PathBuf>,
		format: Format,
	) -> StdResult<ReadySession, Error> {
		let session_builder = setup_base_session(config_mode, home_dir, exec_path, format)?;
		session_builder.build_ready()
	}
}

/// Set up a `SessionBuilder`, with everything but the target.
/// This allows the setup logic to be shared between `hc check`
/// and `hc ready`, since `hc ready` does not use a target.
fn setup_base_session(
	config_mode: ConfigMode,
	home_dir: Option<PathBuf>,
	exec_path: Option<PathBuf>,
	format: Format,
) -> StdResult<SessionBuilder, Error> {
	/*===================================================================
	 *  Setting up the session builder
	 *-----------------------------------------------------------------*/

	let mut session_builder = SessionBuilder::new(chrono::Local::now(), format);

	/*===================================================================
	 *  Loading configuration.
	 *-----------------------------------------------------------------*/

	use ConfigMode::*;
	let config_msg = match config_mode {
		PreferPolicy { policy, config } => match use_policy(policy, &mut session_builder) {
			Err(err) => {
				log::info!("Failed to load default policy KDL file; trying legacy config TOML directory instead. Error: {:#?}", err);

				use_config(config, &mut session_builder)?
			}
			Ok(s) => s,
		},
		ForcePolicy { policy } => use_policy(policy, &mut session_builder)?,
		ForceConfig { config } => use_config(config, &mut session_builder)?,
	};
	Shell::print_config(config_msg);

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

	// Start plugins and display as such to users
	let phase = SpinnerPhase::start("starting plugins");
	phase.inc();
	phase.enable_steady_tick(Duration::from_millis(100));
	let core = start_plugins(&policy, &plugin_cache, executor)?;
	phase.finish_successful();

	session_builder.set_core(core);

	Ok(session_builder)
}

fn use_config(config_path: PathBuf, session_builder: &mut SessionBuilder) -> Result<String> {
	let policy = load_config_and_data(&config_path)?;

	let out = format!(
		"using policy derived from config dir at {}",
		config_path.display()
	);

	// Set config dir
	session_builder
		.set_config_dir(Some(config_path))
		.set_policy(policy)
		.set_policy_path(None);

	Ok(out)
}

fn use_policy(policy_path: PathBuf, session_builder: &mut SessionBuilder) -> Result<String> {
	let policy = load_policy_and_data(&policy_path)?;

	let out = format!("using policy located at {}", policy_path.display());

	// No config dir
	session_builder
		.set_config_dir(None)
		.set_policy(policy)
		.set_policy_path(Some(policy_path));
	Ok(out)
}

pub fn load_config_and_data(config_path: &Path) -> Result<PolicyFile> {
	// Start the phase.
	let phase = SpinnerPhase::start("loading configuration and data files from config file. Note: The use of a config TOML file is deprecated. Please consider using a policy KDL file in the future.");
	// Increment the phase into the "running" stage.
	phase.inc();
	// Set the spinner phase to tick constantly, 10 times a second.
	phase.enable_steady_tick(Duration::from_millis(100));

	// Load the configuration file.
	let config = Config::load_from(config_path)
		.context("Failed to load configuration. If you have not yet done so on this system, try running `hc setup`. Otherwise, please make sure the config files are in the config directory.")?;

	// Convert the Config struct to a PolicyFile struct
	let policy = config_to_policy(config, config_path)?;

	phase.finish_successful();

	Ok(policy)
}

pub fn load_policy_and_data(policy_path: &Path) -> Result<PolicyFile> {
	// Start the phase.
	let phase = SpinnerPhase::start("loading policy and data files");
	// Increment the phase into the "running" stage.
	phase.inc();
	// Set the spinner phase to tick constantly, 10 times a second.
	phase.enable_steady_tick(Duration::from_millis(100));

	// Load the policy file.
	let policy = PolicyFile::load_from(policy_path)
		.with_context(|| format!("Failed to load policy file at path {:?}. Please make sure the policy file is in the provided location and is formatted correctly.", policy_path))?;

	phase.finish_successful();

	Ok(policy)
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
