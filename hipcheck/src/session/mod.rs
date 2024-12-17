// SPDX-License-Identifier: Apache-2.0

pub mod cyclone_dx;
pub mod pm;
pub mod spdx;

use crate::{
	cache::plugin::HcPluginCache,
	cli::Format,
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
	util::command::DependentProgram,
	util::{git::get_git_version, npm::get_npm_version},
	version::{VersionQuery, VersionQueryStorage},
};
use chrono::prelude::*;
use std::{
	env, fmt,
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
	VersionQueryStorage,
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
		config_path: Option<PathBuf>,
		home_dir: Option<PathBuf>,
		policy_path: Option<PathBuf>,
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
		 *  Loading current versions of needed software git, npm, and eslint into salsa.
		 *-----------------------------------------------------------------*/

		let (git_version, npm_version) = load_software_versions()?;

		session.set_git_version(Rc::new(git_version));
		session.set_npm_version(Rc::new(npm_version));

		/*===================================================================
		 *  Loading configuration.
		 *-----------------------------------------------------------------*/

		// Check if a policy file was provided, otherwise convert a deprecated config file to a policy file. If neither was provided, error out.
		if policy_path.is_some() {
			let (policy, policy_path) = load_policy_and_data(policy_path.as_deref())?;

			// No config or dir
			session.set_config_dir(None);

			// Set policy file and its location
			session.set_policy(Rc::new(policy));
			session.set_policy_path(Some(Rc::new(policy_path)));
		} else if config_path.is_some() {
			let (policy, config_dir) = load_config_and_data(config_path.as_deref())?;

			// Set config dir
			session.set_config_dir(Some(Rc::new(config_dir)));

			// Set policy file, with no location to represent that none was given
			session.set_policy(Rc::new(policy));
			session.set_policy_path(None);
		} else {
			return Err(hc_error!("No policy file or (deprecated) config file found. Please provide a policy file before running Hipcheck."));
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
		 *  Resolving the Hipcheck version.
		 *-----------------------------------------------------------------*/

		let raw_version = env!("CARGO_PKG_VERSION", "can't find Hipcheck package version");
		session.set_hc_version(Rc::new(raw_version.to_string()));

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

fn load_software_versions() -> Result<(String, String)> {
	let git_version = get_git_version()?;
	DependentProgram::Git.check_version(&git_version)?;

	let npm_version = get_npm_version()?;
	DependentProgram::Npm.check_version(&npm_version)?;

	Ok((git_version, npm_version))
}

pub fn load_config_and_data(config_path: Option<&Path>) -> Result<(PolicyFile, PathBuf)> {
	// Start the phase.
	let phase = SpinnerPhase::start("Loading configuration and data files from config file. Note: The use of a config TOML file is deprecated. Please consider using a policy KDL file in the future.");
	// Increment the phase into the "running" stage.
	phase.inc();
	// Set the spinner phase to tick constantly, 10 times a second.
	phase.enable_steady_tick(Duration::from_millis(100));

	// Resolve the path to the config file.
	let valid_config_path = config_path
	   .ok_or_else(|| hc_error!("Failed to load configuration. Please make sure the path set by the hc_config env variable exists."))?;

	// Load the configuration file.
	let config = Config::load_from(valid_config_path)
		.context("Failed to load configuration. If you have not yet done so on this system, try running `hc setup`. Otherwise, please make sure the config files are in the config directory.")?;

	// Convert the Config struct to a PolicyFile struct
	let policy = config_to_policy(config)?;

	phase.finish_successful();

	Ok((policy, valid_config_path.to_path_buf()))
}

pub fn load_policy_and_data(policy_path: Option<&Path>) -> Result<(PolicyFile, PathBuf)> {
	// Start the phase.
	let phase = SpinnerPhase::start("loading policy and data files");
	// Increment the phase into the "running" stage.
	phase.inc();
	// Set the spinner phase to tick constantly, 10 times a second.
	phase.enable_steady_tick(Duration::from_millis(100));

	// Resolve the path to the policy file.
	let valid_policy_path = policy_path.ok_or_else(|| {
		hc_error!(
			"Failed to load policy. Please make sure the path set by the --policy flag exists."
		)
	})?;

	// Load the policy file.
	let policy = PolicyFile::load_from(valid_policy_path)
		.context("Failed to load policy. Please make sure the policy file is in the provided location and is formatted correctly.")?;

	phase.finish_successful();

	Ok((policy, valid_policy_path.to_path_buf()))
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
