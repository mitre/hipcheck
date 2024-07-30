// SPDX-License-Identifier: Apache-2.0

// mod pm;
// mod spdx;

use crate::analysis::score::ScoringProviderStorage;
use crate::analysis::AnalysisProviderStorage;
use crate::command_util::DependentProgram;
use crate::config::AttacksConfigQueryStorage;
use crate::config::CommitConfigQueryStorage;
use crate::config::Config;
use crate::config::ConfigSource;
use crate::config::ConfigSourceStorage;
use crate::config::FuzzConfigQueryStorage;
use crate::config::LanguagesConfigQueryStorage;
use crate::config::PracticesConfigQueryStorage;
use crate::config::RiskConfigQueryStorage;
use crate::config::WeightTreeQueryStorage;
use crate::context::Context as _;
use crate::data::git::get_git_version;
use crate::data::git::GitProviderStorage;
use crate::data::npm::get_npm_version;
use crate::data::CodeQualityProviderStorage;
use crate::data::DependenciesProviderStorage;
use crate::data::FuzzProviderStorage;
use crate::data::GitHubProviderStorage;
use crate::data::ModuleProvider;
use crate::data::ModuleProviderStorage;
use crate::data::PullRequestReviewProviderStorage;
use crate::error::Error;
use crate::error::Result;
use crate::hc_error;
use crate::metric::binary_detector::BinaryFileStorage;
use crate::metric::linguist::LinguistStorage;
use crate::metric::MetricProviderStorage;
use crate::report::Format;
use crate::report::ReportParams;
use crate::report::ReportParamsStorage;
use crate::session::pm::detect_and_extract;
use crate::session::spdx::extract_spdx_download_url;
use crate::shell::spinner_phase::SpinnerPhase;
use crate::shell::Shell;
use crate::source::source;
use crate::source::source::SourceQuery;
use crate::source::source::SourceQueryStorage;
use crate::target::SbomStandard;
use crate::target::{Target, TargetSeed};
use crate::version::get_version;
use crate::version::VersionQuery;
use crate::version::VersionQueryStorage;
use chrono::prelude::*;
use dotenv::var;
use std::fmt;
use std::path::Path;
use std::path::PathBuf;
use std::rc::Rc;
use std::result::Result as StdResult;
use std::sync::Arc;
use std::time::Duration;
use url::Url;

use super::cyclone_dx::extract_cyclonedx_download_url;
use super::pm::extract_repo_for_maven;

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
	LanguagesConfigQueryStorage,
	LinguistStorage,
	MetricProviderStorage,
	ModuleProviderStorage,
	FuzzConfigQueryStorage,
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
		format: Format,
		raw_version: &str,
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

		let (config, config_dir, data_dir, hc_github_token) =
			match load_config_and_data(config_path.as_deref(), data_path.as_deref()) {
				Ok(results) => results,
				Err(err) => return Err(err),
			};

		// Set config input queries for use below
		session.set_config(Rc::new(config));
		session.set_config_dir(Rc::new(config_dir));

		// Set data folder location for module analysis
		session.set_data_dir(Arc::new(data_dir));

		// Set github token in salsa
		session.set_github_api_token(Some(Rc::new(hc_github_token)));

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

		let version = match get_version(raw_version) {
			Ok(version) => version,
			Err(err) => return Err(err),
		};

		session.set_hc_version(Rc::new(version));

		/*===================================================================
		 *  Remaining input queries.
		 *-----------------------------------------------------------------*/

		// Set remaining input queries
		session.set_format(format);
		session.set_started_at(Local::now().into());

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
) -> Result<(Config, PathBuf, PathBuf, String)> {
	// Start the phase.
	let phase = SpinnerPhase::start("loading configuration and data files");
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

	// Get the directory the data file is in.
	let data_dir = data_path
	   .ok_or_else(|| hc_error!("Failed to load data files. Please make sure the path set by the hc_data env variable exists."))?
		.to_owned();

	// Resolve the github token file.
	let hc_github_token = resolve_token()?;

	phase.finish_successful();

	Ok((
		config,
		valid_config_path.to_path_buf(),
		data_dir,
		hc_github_token,
	))
}

fn load_target(seed: &TargetSeed, home: &Path) -> Result<Target> {
	// Resolve the source specifier into an actual source.
	let phase_desc = match seed {
		TargetSeed::LocalRepo(_) | TargetSeed::RemoteRepo(_) => "resolving git repository target",
		TargetSeed::Package(_) => "resolving package target",
		TargetSeed::Sbom(_) => "parsing SBOM document",
		TargetSeed::MavenPackage(_) => "resolving maven package target",
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
	#[cfg(feature = "print-timings")]
	let _0 = crate::benchmarking::print_scope_time!("resolve_source");

	match seed {
		TargetSeed::RemoteRepo(repo) => source::resolve_remote_repo(phase, home, repo.to_owned()),
		TargetSeed::LocalRepo(source) => source::resolve_local_repo(phase, home, source.to_owned()),
		TargetSeed::Package(package) => {
			// Attempt to get the git repo URL for the package
			let package_git_repo_url =
				detect_and_extract(package).context("Could not get git repo URL for package")?;

			// Create Target for a remote git repo originating with a package
			let package_git_repo = source::get_remote_repo_from_url(package_git_repo_url)?;
			source::resolve_remote_package_repo(
				phase,
				home,
				package_git_repo,
				format!("{}@{}", package.name, package.version),
			)
		}
		TargetSeed::MavenPackage(package) => {
			// Attempt to get the git repo URL for the Maven package
			let package_git_repo_url = extract_repo_for_maven(package.url.as_ref())
				.context("Could not get git repo URL for Maven package")?;

			// Create Target for a remote git repo originating with a Maven package
			let package_git_repo = source::get_remote_repo_from_url(package_git_repo_url)?;
			source::resolve_remote_package_repo(
				phase,
				home,
				package_git_repo,
				package.url.to_string(),
			)
		}
		TargetSeed::Sbom(sbom) => {
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
			source::resolve_remote_package_repo(phase, home, sbom_git_repo, source.to_string())
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::test_util::with_env_vars;

	#[test]
	fn resolve_token_test() {
		let vars = vec![("HC_GITHUB_TOKEN", Some("test"))];
		with_env_vars(vars, || assert_eq!(resolve_token().unwrap(), "test"));
	}
}
