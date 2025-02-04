// SPDX-License-Identifier: Apache-2.0

use crate::{
	cache::plugin::HcPluginCache,
	cli::{ConfigMode, Format},
	config::{
		Config, ConfigSource, ConfigSourceStorage, RiskConfigQuery, RiskConfigQueryStorage,
		WeightTreeQueryStorage,
	},
	engine::{start_plugins, HcEngine, HcEngineStorage},
	error::{Context as _, Error, Result},
	exec::ExecConfig,
	hc_error,
	policy::{config_to_policy, PolicyFile},
	report::{ReportParams, ReportParamsStorage},
	score::ScoringProviderStorage,
	shell::{spinner_phase::SpinnerPhase, Shell},
	source::{SourceQuery, SourceQueryStorage},
	target::{
		resolve::{TargetResolver, TargetResolverConfig},
		Target, TargetSeed, TargetSeedKind,
	},
};
use chrono::prelude::*;
use std::{
	fmt,
	path::{Path, PathBuf},
	rc::Rc,
	result::Result as StdResult,
	sync::Arc,
	time::Duration,
};

/// Immutable configuration and base data for a run of Hipcheck.
#[salsa::database(
	ConfigSourceStorage,
	HcEngineStorage,
	ReportParamsStorage,
	RiskConfigQueryStorage,
	ScoringProviderStorage,
	SourceQueryStorage,
	WeightTreeQueryStorage
)]
pub struct Session {
	// Query storage.
	storage: salsa::Storage<Self>,
}

// Required by our query groups
impl salsa::Database for Session {}

// Cannot be derived because `salsa::Storage` does not implement it
impl fmt::Debug for Session {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "Session {{ storage: salsa::Storage<Session> }}")
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
		 *  Setting up the session.
		 *-----------------------------------------------------------------*/

		// Input query setters are implemented on `Session`, not
		// `salsa::Storage<Session>`
		let mut session = Session {
			storage: Default::default(),
		};

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
				let res = use_policy(policy, &mut session);
				if let Some(err) = res.err() {
					log::error!("Failed to load default policy KDL file; trying legacy config TOML directory instead. Error: {:#?}", err);

					use_config(config, &mut session)?;
				}
			}
			ForcePolicy { policy } => {
				use_policy(policy, &mut session)?;
			}
			ForceConfig { config } => {
				use_config(config, &mut session)?;
			}
		}

		// Force eval the risk policy expr - wouldn't be necessary if the PolicyFile parsed
		let _ = session.risk_policy()?;

		/*===================================================================
		 *  Load the Exec Configuration
		 *-----------------------------------------------------------------*/
		let exec = load_exec_config(exec_path.as_deref())?;

		session.set_exec_config(Rc::new(exec));

		/*===================================================================
		 *  Resolving the Hipcheck home.
		 *-----------------------------------------------------------------*/

		let home = home_dir
			.as_deref()
			.map(ToOwned::to_owned)
			.ok_or_else(|| hc_error!("can't find cache directory"))?;

		session.set_cache_dir(Rc::new(home.clone()));

		let plugin_cache = HcPluginCache::new(&home);

		/*===================================================================
		 *  Resolving the source.
		 *-----------------------------------------------------------------*/

		let target = load_target(target, &home)?;
		session.set_target(Arc::new(target));

		/*===================================================================
		 *  Remaining input queries.
		 *-----------------------------------------------------------------*/

		// Set remaining input queries
		session.set_format(format);
		session.set_started_at(Local::now().into());

		/*===================================================================
		 *  Plugin startup.
		 *-----------------------------------------------------------------*/

		// The fact that we set the policy above to be accessible through the salsa
		// infrastructure would suggest that plugin startup should also be done
		// through salsa. Our goal is to produce an HcPluginCore instance, which
		// has all the plugins up and running. However, HcPluginCore does not impl
		// equal, and the idea of memoizing/invalidating it does not make sense.
		// Thus, we will do the plugin startup here.
		let policy = session.policy();

		let exec_config = session.exec_config();

		let executor = ExecConfig::get_plugin_executor(&exec_config)?;

		let core = start_plugins(policy.as_ref(), &plugin_cache, executor)?;
		session.set_core(core);

		Ok(session)
	}
}

fn use_config(config_path: PathBuf, session: &mut Session) -> Result<()> {
	let (policy, config_dir) = load_config_and_data(config_path)?;

	// Set config dir
	session.set_config_dir(Some(Rc::new(config_dir)));

	// Set policy file, with no location to represent that none was given
	session.set_policy(Rc::new(policy));
	session.set_policy_path(None);
	Ok(())
}

fn use_policy(policy_path: PathBuf, session: &mut Session) -> Result<()> {
	let (policy, policy_path) = load_policy_and_data(policy_path)?;

	// No config or dir
	session.set_config_dir(None);

	// Set policy file and its location
	session.set_policy(Rc::new(policy));
	session.set_policy_path(Some(Rc::new(policy_path)));
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
