// SPDX-License-Identifier: Apache-2.0

pub mod cyclone_dx;
pub mod pm;
pub mod spdx;

use crate::{
	analysis::{score::ScoringProviderStorage, AnalysisProviderStorage},
	cache::plugin_cache::HcPluginCache,
	cli::Format,
	command_util::DependentProgram,
	config::{
		AttacksConfigQueryStorage, CommitConfigQueryStorage, Config, ConfigSource,
		ConfigSourceStorage, LanguagesConfigQueryStorage, PracticesConfigQueryStorage,
		RiskConfigQueryStorage, WeightTreeQueryStorage,
	},
	context::Context as _,
	data::{
		git::{get_git_version, GitProviderStorage},
		npm::get_npm_version,
		CodeQualityProviderStorage, DependenciesProviderStorage, FuzzProviderStorage,
		GitHubProviderStorage, ModuleProvider, ModuleProviderStorage,
		PullRequestReviewProviderStorage,
	},
	engine::{start_plugins, HcEngine, HcEngineStorage},
	error::{Error, Result},
	hc_error,
	metric::{
		binary_detector::BinaryFileStorage, linguist::LinguistStorage, MetricProviderStorage,
	},
	policy::{config_to_policy::config_to_policy, PolicyFile},
	report::{ReportParams, ReportParamsStorage},
	session::{
		cyclone_dx::extract_cyclonedx_download_url,
		pm::{detect_and_extract, extract_repo_for_maven},
		spdx::extract_spdx_download_url,
	},
	shell::{spinner_phase::SpinnerPhase, Shell},
	source,
	source::{SourceQuery, SourceQueryStorage},
	target::{SbomStandard, Target, TargetSeed, TargetSeedKind},
	version::{VersionQuery, VersionQueryStorage},
};
use chrono::prelude::*;
use dotenv::var;
use std::{
	fmt,
	path::{Path, PathBuf},
	rc::Rc,
	result::Result as StdResult,
	sync::Arc,
	time::Duration,
};
use url::Url;

/// Immutable configuration and base data for a run of Hipcheck.
#[salsa::database(
	AnalysisProviderStorage,
	AttacksConfigQueryStorage,
	BinaryFileStorage,
	CodeQualityProviderStorage,
	CommitConfigQueryStorage,
	ConfigSourceStorage,
	DependenciesProviderStorage,
	GitProviderStorage,
	GitHubProviderStorage,
	HcEngineStorage,
	LanguagesConfigQueryStorage,
	LinguistStorage,
	MetricProviderStorage,
	ModuleProviderStorage,
	FuzzProviderStorage,
	PracticesConfigQueryStorage,
	PullRequestReviewProviderStorage,
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
		data_path: Option<PathBuf>,
		home_dir: Option<PathBuf>,
		policy_path: Option<PathBuf>,
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

		let (git_version, npm_version) = match load_software_versions() {
			Ok(results) => results,
			Err(err) => return Err(err),
		};

		session.set_git_version(Rc::new(git_version));
		session.set_npm_version(Rc::new(npm_version));

		/*===================================================================
		 *  Loading configuration.
		 *-----------------------------------------------------------------*/

		// Check if a policy file was provided, otherwise convert a deprecated config file to a policy file. If neither was provided, error out.
		if policy_path.is_some() {
			let (policy, policy_path, data_dir, hc_github_token) =
				match load_policy_and_data(policy_path.as_deref(), data_path.as_deref()) {
					Ok(results) => results,
					Err(err) => return Err(err),
				};

			// No config or dir
			session.set_config_dir(None);

			// Set policy file and its location
			session.set_policy(Rc::new(policy));
			session.set_policy_path(Some(Rc::new(policy_path)));

			// Set data folder location for module analysis
			session.set_data_dir(Arc::new(data_dir));

			// Set github token in salsa
			session.set_github_api_token(Some(Rc::new(hc_github_token)));
		} else if config_path.is_some() {
			let (policy, config_dir, data_dir, hc_github_token) =
				match load_config_and_data(config_path.as_deref(), data_path.as_deref()) {
					Ok(results) => results,
					Err(err) => return Err(err),
				};

			// Set config dir
			session.set_config_dir(Some(Rc::new(config_dir)));

			// Set policy file, with no location to represent that none was given
			session.set_policy(Rc::new(policy));
			session.set_policy_path(None);

			// Set data folder location for module analysis
			session.set_data_dir(Arc::new(data_dir));

			// Set github token in salsa
			session.set_github_api_token(Some(Rc::new(hc_github_token)));
		} else {
			return Err(hc_error!("No policy file or (deprecated) config file found. Please provide a policy file before running Hipcheck."));
		}

		/*===================================================================
		 *  Resolving the Hipcheck home.
		 *-----------------------------------------------------------------*/

		let home = match home_dir
			.as_deref()
			.map(ToOwned::to_owned)
			.ok_or_else(|| hc_error!("can't find cache directory"))
		{
			Ok(results) => results,
			Err(err) => return Err(err),
		};

		session.set_cache_dir(Rc::new(home.clone()));

		let plugin_cache = HcPluginCache::new(&home);

		/*===================================================================
		 *  Resolving the source.
		 *-----------------------------------------------------------------*/

		let target = match load_target(target, &home) {
			Ok(results) => results,
			Err(err) => return Err(err),
		};

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
		let core = start_plugins(policy.as_ref(), &plugin_cache)?;
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

fn load_config_and_data(
	config_path: Option<&Path>,
	data_path: Option<&Path>,
) -> Result<(PolicyFile, PathBuf, PathBuf, String)> {
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

	// Get the directory the data file is in.
	let data_dir = data_path
	   .ok_or_else(|| hc_error!("Failed to load data files. Please make sure the path set by the hc_data env variable exists."))?
		.to_owned();

	// Resolve the github token file.
	let hc_github_token = resolve_token()?;

	phase.finish_successful();

	Ok((
		policy,
		valid_config_path.to_path_buf(),
		data_dir,
		hc_github_token,
	))
}

fn load_policy_and_data(
	policy_path: Option<&Path>,
	data_path: Option<&Path>,
) -> Result<(PolicyFile, PathBuf, PathBuf, String)> {
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
		.context("Failed to load policy. Plase make sure the policy file is in the proidved location and is formatted correctly.")?;

	// Get the directory the data file is in.
	let data_dir = data_path
	   .ok_or_else(|| hc_error!("Failed to load data files. Please make sure the path set by the hc_data env variable exists."))?
		.to_owned();

	// Resolve the github token file.
	let hc_github_token = resolve_token()?;

	phase.finish_successful();

	Ok((
		policy,
		valid_policy_path.to_path_buf(),
		data_dir,
		hc_github_token,
	))
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

/// Resolves github token for Hipcheck to query github with.
fn resolve_token() -> Result<String> {
	match var("HC_GITHUB_TOKEN") {
		Ok(token) => Ok(token),
		_ => Ok("".to_string()),
	}
}

/// Resolves the target specifier into an actual target.
fn resolve_target(seed: &TargetSeed, phase: &SpinnerPhase, home: &Path) -> Result<Target> {
	use TargetSeedKind::*;
	#[cfg(feature = "print-timings")]
	let _0 = crate::benchmarking::print_scope_time!("resolve_source");

	match &seed.kind {
		RemoteRepo(remote) => {
			source::resolve_remote_repo(phase, home, remote.to_owned(), seed.refspec.clone())
		}
		LocalRepo(source) => {
			// Because other TargetSeedKind variants need to transfer refspec info from the CLI,
			// there's overlap with LocalGitRepo.git_ref. Copy CLI refspec here.
			let mut source = source.to_owned();
			source.git_ref = seed.refspec.clone().unwrap_or("HEAD".to_owned());
			source::resolve_local_repo(phase, home, source)
		}
		Package(package) => {
			// Attempt to get the git repo URL for the package
			let package_git_repo_url =
				detect_and_extract(package).context("Could not get git repo URL for package")?;

			// Create Target for a remote git repo originating with a package
			let package_git_repo = source::get_remote_repo_from_url(package_git_repo_url)?;
			// TargetSeed validation step should have already ensured both refspec and package
			// version are not provided, so we can do this
			let refspec = if let Some(refspec) = &seed.refspec {
				Some(refspec.to_owned())
			} else if package.has_version() {
				Some(package.version.to_owned())
			} else {
				None
			};
			source::resolve_remote_package_repo(
				phase,
				home,
				package_git_repo,
				format!("{}@{}", package.name, package.version),
				refspec,
			)
		}
		MavenPackage(package) => {
			// Attempt to get the git repo URL for the Maven package
			let package_git_repo_url = extract_repo_for_maven(package.url.as_ref())
				.context("Could not get git repo URL for Maven package")?;

			// Create Target for a remote git repo originating with a Maven package
			let package_git_repo = source::get_remote_repo_from_url(package_git_repo_url)?;
			// We do not currently harvest version info from the maven url
			source::resolve_remote_package_repo(
				phase,
				home,
				package_git_repo,
				package.url.to_string(),
				seed.refspec.clone(),
			)
		}
		Sbom(sbom) => {
			let source = sbom.path.to_str().ok_or(hc_error!(
				"SBOM path contained one or more invalid characters"
			))?;
			// Attempt to get the download location for the local SBOM package, using the function
			// appropriate to the SBOM standard
			let download_url = match sbom.standard {
				SbomStandard::Spdx => Url::parse(&extract_spdx_download_url(source)?)?,
				SbomStandard::CycloneDX => extract_cyclonedx_download_url(source)?,
			};

			// Create a Target for a remote git repo originating with an SBOM
			let sbom_git_repo = source::get_remote_repo_from_url(download_url)?;
			source::resolve_remote_package_repo(
				phase,
				home,
				sbom_git_repo,
				source.to_string(),
				seed.refspec.clone(),
			)
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::util::test::with_env_vars;

	#[test]
	fn resolve_token_test() {
		let vars = vec![("HC_GITHUB_TOKEN", Some("test"))];
		with_env_vars(vars, || assert_eq!(resolve_token().unwrap(), "test"));
	}
}
