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
use crate::session::spdx::extract_download_url;
use crate::shell::Phase;
use crate::shell::Shell;
use crate::source::source::Source;
use crate::source::source::SourceChangeRequest;
use crate::source::source::SourceKind;
use crate::source::source::SourceQuery;
use crate::source::source::SourceQueryStorage;
use crate::source::source::SourceRepo;
use crate::version::get_version;
use crate::version::VersionQuery;
use crate::version::VersionQueryStorage;
use crate::CheckKind;
use chrono::prelude::*;
use dotenv::var;
use std::fmt;
use std::path::Path;
use std::path::PathBuf;
use std::rc::Rc;
use std::result::Result as StdResult;
use std::sync::Arc;

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
	VersionQueryStorage
)]
pub struct Session {
	/// The shell interface, for outputting progress.
	pub shell: Shell,

	// Query storage.
	storage: salsa::Storage<Self>,
}

// Required by our query groups
impl salsa::Database for Session {}

// Cannot be derived because `salsa::Storage` does not implement it
impl fmt::Debug for Session {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(
			f,
			"Session {{ shell: {:?}, storage: salsa::Storage<Session> }}",
			self.shell
		)
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
		shell: Shell,
		source_type: &Check,
		source: &str,
		config_path: Option<PathBuf>,
		data_path: Option<PathBuf>,
		home_dir: Option<PathBuf>,
		format: Format,
		raw_version: &str,
	) -> StdResult<Session, (Shell, Error)> {
		/*===================================================================
		 *  Setting up the session.
		 *-----------------------------------------------------------------*/

		// Input query setters are implemented on `Session`, not
		// `salsa::Storage<Session>`
		let mut session = Session {
			shell,
			storage: Default::default(),
		};

		/*===================================================================
		 * Printing the prelude.
		 *-----------------------------------------------------------------*/

		if let Err(err) = session.shell.prelude(source) {
			return Err((session.shell, err));
		};

		/*===================================================================
		 *  Loading current versions of needed software git, npm, and eslint into salsa.
		 *-----------------------------------------------------------------*/

		let (git_version, npm_version) = match load_software_versions(&mut session.shell) {
			Ok(results) => results,
			Err(err) => return Err((session.shell, err)),
		};

		session.set_git_version(Rc::new(git_version));
		session.set_npm_version(Rc::new(npm_version));

		/*===================================================================
		 *  Loading configuration.
		 *-----------------------------------------------------------------*/

		let (config, config_dir, data_dir, hc_github_token) = match load_config_and_data(
			&mut session.shell,
			config_path.as_deref(),
			data_path.as_deref(),
		) {
			Ok(results) => results,
			Err(err) => return Err((session.shell, err)),
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
			Err(err) => return Err((session.shell, err)),
		};

		/*===================================================================
		 *  Resolving the source.
		 *-----------------------------------------------------------------*/

		let source = match load_source(&mut session.shell, source, source_type, &home) {
			Ok(results) => results,
			Err(err) => return Err((session.shell, err)),
		};

		session.set_source(Arc::new(source));

		/*===================================================================
		 *  Resolving the Hipcheck version.
		 *-----------------------------------------------------------------*/

		let version = match get_version(raw_version) {
			Ok(version) => version,
			Err(err) => return Err((session.shell, err)),
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

	/// Consume self and get the `Shell` back out.
	///
	/// This is used so any error printing at the end of the session can still go
	/// through the shell interface.
	pub fn end(self) -> Shell {
		self.shell
	}
}

fn load_software_versions(_shell: &mut Shell) -> Result<(String, String)> {
	let git_version = get_git_version()?;
	DependentProgram::Git.check_version(&git_version)?;

	let npm_version = get_npm_version()?;
	DependentProgram::Npm.check_version(&npm_version)?;

	Ok((git_version, npm_version))
}

fn load_config_and_data(
	shell: &mut Shell,
	config_path: Option<&Path>,
	data_path: Option<&Path>,
) -> Result<(Config, PathBuf, PathBuf, String)> {
	// Start the phase.
	let phase = shell.phase("loading configuration and data files")?;

	// Resolve the path to the config file.
	let valid_config_path = config_path
	   .ok_or_else(|| hc_error!("Failed to load configuration. Please make sure the path set by the hc_config env variable exists."))?;

	// Load the configuration file.
	let config = Config::load_from(valid_config_path)
		.context("Failed to load configuration. Please make sure the config files are in the config directory.")?;

	// Get the directory the data file is in.
	let data_dir = data_path
	   .ok_or_else(|| hc_error!("Failed to load data files. Please make sure the path set by the hc_data env variable exists."))?
		.to_owned();

	// Resolve the github token file.
	let hc_github_token = resolve_token()?;

	phase.finish()?;

	Ok((
		config,
		valid_config_path.to_path_buf(),
		data_dir,
		hc_github_token,
	))
}

fn load_source(
	shell: &mut Shell,
	source: &str,
	source_type: &Check,
	home: &Path,
) -> Result<Source> {
	// Resolve the source specifier into an actual source.
	let phase_desc = match source_type.kind.target_kind() {
		TargetKind::RepoSource => "resolving git repository source",
		TargetKind::PrUri => "resolving git pull request source",
		TargetKind::PackageVersion => "resolving package source",
		TargetKind::SpdxDocument => "parsing SPDX document",
		_ => return Err(hc_error!("source specified was not a valid source")),
	};

	let mut phase = shell.phase(phase_desc)?;
	let source = resolve_source(source_type, &mut phase, home, source)?;
	phase.finish()?;

	Ok(source)
}

/// Resolves github token for Hipcheck to query github with.
fn resolve_token() -> Result<String> {
	match var("HC_GITHUB_TOKEN") {
		Ok(token) => Ok(token),
		_ => Ok("".to_string()),
	}
}

/// Resolves the source specifier into an actual source.
fn resolve_source(
	source_type: &Check,
	phase: &mut Phase,
	home: &Path,
	source: &str,
) -> Result<Source> {
	#[cfg(feature = "print-timings")]
	let _0 = crate::benchmarking::print_scope_time!("resolve_source");

	match source_type.kind.target_kind() {
		TargetKind::RepoSource => {
			SourceRepo::resolve_repo(phase, home, source).map(|repo| Source {
				kind: SourceKind::Repo(repo),
			})
		}
		TargetKind::PackageVersion => {
			let package = source;

			let command = &source_type.to_owned().kind;

			let package_git_repo_url = detect_and_extract(package, command.name().to_owned())
				.context("Could not get git repo URL for package")?;

			SourceRepo::resolve_repo(phase, home, package_git_repo_url.as_str()).map(|repo| {
				Source {
					kind: SourceKind::Repo(repo),
				}
			})
		}
		TargetKind::PrUri => {
			SourceChangeRequest::resolve_change_request(phase, home, source).map(|cr| Source {
				kind: SourceKind::ChangeRequest(cr),
			})
		}
		TargetKind::SpdxDocument => {
			let download_url = extract_download_url(source)?;
			SourceRepo::resolve_repo(phase, home, &download_url).map(|repo| Source {
				kind: SourceKind::Repo(repo),
			})
		}
		_ => Err(Error::msg("source specified was not a valid source")),
	}
}

pub struct Check {
	pub target: String,
	pub kind: CheckKind,
}

#[derive(Debug, PartialEq, Eq)]
pub enum TargetKind {
	RepoSource,
	PrUri,
	PatchUri,
	PackageVersion,
	SpdxDocument,
}

impl TargetKind {
	pub fn is_checkable(&self) -> bool {
		use TargetKind::*;

		match self {
			RepoSource | PrUri | PackageVersion | SpdxDocument => true,
			PatchUri => false,
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
