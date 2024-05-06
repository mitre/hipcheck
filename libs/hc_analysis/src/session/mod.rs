// SPDX-License-Identifier: Apache-2.0

mod pm;
mod spdx;

use crate::{
	metric::{
		binary_detector::BinaryFileStorage, linguist::LinguistStorage, MetricProviderStorage,
	},
	score::ScoringProviderStorage,
	AnalysisProviderStorage,
};
use dotenv::var;
use hc_common::context::Context as _;
use hc_common::{
	chrono::prelude::*,
	command_util::DependentProgram,
	config::{
		AttacksConfigQueryStorage, CommitConfigQueryStorage, Config, ConfigSource,
		ConfigSourceStorage, FuzzConfigQueryStorage, LanguagesConfigQueryStorage,
		PracticesConfigQueryStorage, RiskConfigQueryStorage,
	},
	error::{Error, Result},
	filesystem::create_dir_all,
	hc_error, pathbuf, salsa,
	version::{get_version, VersionQuery, VersionQueryStorage},
	HIPCHECK_TOML_FILE,
};
use hc_data::ModuleProvider;
use hc_data::{
	git::get_git_version,
	git::GitProviderStorage,
	npm::get_npm_version,
	source::{
		Source, SourceChangeRequest, SourceKind, SourceQuery, SourceQueryStorage, SourceRepo,
	},
	CodeQualityProviderStorage, DependenciesProviderStorage, FuzzProviderStorage,
	GitHubProviderStorage, ModuleProviderStorage, PullRequestReviewProviderStorage,
};
use hc_report::{Format, ReportParams, ReportParamsStorage};
use hc_shell::{Phase, Shell};
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::result::Result as StdResult;

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
		source: &OsStr,
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

		if let Err(err) = session.shell.prelude(source.to_string_lossy().as_ref()) {
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
		session.set_data_dir(Rc::new(data_dir));

		// Set github token in salsa
		session.set_github_api_token(Some(Rc::new(hc_github_token)));

		/*===================================================================
		 *  Resolving the Hipcheck home.
		 *-----------------------------------------------------------------*/

		let home = match load_home(&mut session.shell, home_dir.as_deref()) {
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

		session.set_source(Rc::new(source));

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
	let valid_config_path = resolve_config(config_path)
		.context("Failed to load configuration. Please make sure the path set by the hc_config env variable exists.")?;

	// Get the directory the config file is in.
	let config_dir = valid_config_path
		.parent()
		.map(ToOwned::to_owned)
		.ok_or_else(|| hc_error!("can't identify directory of config file"))?;

	// Load the configuration file.
	let config = Config::load_from(&valid_config_path)
		.context("Failed to load configuration. Please make sure the config files are in the config directory.")?;

	// Get the directory the data file is in.
	let data_dir = resolve_data(data_path)
		.context("Failed to load data files. Please make sure the path set by the hc_data env variable exists.")?;

	// Resolve the github token file.
	let hc_github_token = resolve_token()?;

	phase.finish()?;

	Ok((config, config_dir, data_dir, hc_github_token))
}

fn load_home(_shell: &mut Shell, home_dir: Option<&Path>) -> Result<PathBuf> {
	// If no env or dotenv vars set, return error as Hipcheck can not run without config set
	let home = resolve_home(home_dir)?;

	Ok(home)
}

fn load_source(
	shell: &mut Shell,
	source: &OsStr,
	source_type: &Check,
	home: &Path,
) -> Result<Source> {
	// Resolve the source specifier into an actual source.
	let phase_desc = match source_type.check_type {
		CheckType::RepoSource => "resolving git repository source",
		CheckType::PrUri => "resolving git pull request source",
		CheckType::PackageVersion => "resolving package source",
		CheckType::SpdxDocument => "parsing SPDX document",
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

/// Resolves a home location for Hipcheck to cache data.
pub fn resolve_home(home_flag: Option<&Path>) -> Result<PathBuf> {
	// 1. Prefer --home flag if it is set use home_dir parameter
	// 2. Prefer HC_HOME if it is set in env or .env file.
	// 3. Otherwise, use cross platform cache directory as a default.
	//        `$XDG_CACHE_HOME` or `$HOME/.cache` on Linux,
	//        `$HOME/Library/Caches` on macOS,
	//        `{FOLDERID_LocalAppData}` on Windows
	//    (See https://docs.rs/dirs/3.0.2/dirs/fn.cache_dir.html)

	if let Some(home_dir) = home_flag {
		if home_dir.exists() {
			return Ok(home_dir.to_owned());
		}

		return Err(hc_error!(
			"home directory {} (from --home) does not exist",
			home_dir.display()
		));
	}

	if let Ok(home_dir) = dotenv::var("HC_HOME").map(PathBuf::from) {
		if home_dir.exists() {
			return Ok(home_dir);
		}

		return Err(hc_error!(
			"home directory {} (from HC_HOME) does not exist",
			home_dir.display()
		));
	}

	if let Some(cache_dir) = dirs::cache_dir() {
		// We should always be fine to create the cache directory if it doesn't exist already.
		let home_dir = pathbuf![&cache_dir, "hipcheck"];

		create_dir_all(&home_dir).context("failed to create Hipcheck home directory")?;

		return Ok(home_dir);
	}

	Err(hc_error!("can't find Hipcheck home (try setting the `HC_HOME` environment variable or `--home <DIR>` flag)"))
}

/// Resolves a config folder location for Hipcheck to to find config files in
pub fn resolve_config(config_flag: Option<&Path>) -> Result<PathBuf> {
	// 1. Prefer --config flag parameter if it exists as path
	// 2. Prefer HC_CONFIG if it is set in env or .env file.
	// 3. Otherwise, use cross platform cache directory as a default.
	//        `$XDG_CONFIG_HOME` or `$HOME/.config` on Linux,
	//        `$HOME/Library/Application Support` on macOS,
	//        `{FOLDERID_RoamingAppData}` on Windows
	//    (See https://docs.rs/dirs/3.0.2/dirs/fn.cache_dir.html)

	if let Some(config_path) = config_flag {
		let full_config_path = pathbuf![&config_path, HIPCHECK_TOML_FILE];
		if full_config_path.exists() {
			return Ok(full_config_path);
		}

		return Err(hc_error!(
			"config file {} (from --config) does not exist",
			full_config_path.display()
		));
	}

	if let Ok(config_path) = dotenv::var("HC_CONFIG").map(PathBuf::from) {
		let full_config_path = pathbuf![&config_path, HIPCHECK_TOML_FILE];
		if full_config_path.exists() {
			return Ok(full_config_path);
		}

		return Err(hc_error!(
			"config file {} (from HC_CONFIG) does not exist",
			full_config_path.display()
		));
	}

	if let Some(config_dir) = dirs::config_dir() {
		if config_dir.exists() {
			let config_path = pathbuf![&config_dir, "hipcheck", HIPCHECK_TOML_FILE];

			if config_path.exists() {
				return Ok(config_path);
			}
		}
	}

	Err(hc_error!("can't find config (try setting the `HC_CONFIG` environment variable or `--config <FILE>` flag)"))
}

/// Resolves a data folder location for Hipcheck to to find data files in
pub fn resolve_data(data_flag: Option<&Path>) -> Result<PathBuf> {
	// 1. Prefer --data flag parameter if it exists as path
	// 2. Prefer HC_DATA if it is set in env or .env file.
	// 3. Otherwise, use cross platform data directory as a default.
	//        `$XDG_DATA_HOME` or `$HOME/.local/share` on Linux,
	//        `$HOME`/Library/Application Support` on macOS,
	//        `{FOLDERID_RoamingAppData}` on Windows
	//    (See https://docs.rs/dirs/3.0.2/dirs/fn.cache_dir.html)

	if let Some(data_path) = data_flag {
		if data_path.exists() {
			return Ok(data_path.to_owned());
		}

		return Err(hc_error!(
			"data file {} (from --data) does not exist",
			data_path.display()
		));
	}

	if let Ok(data_path) = dotenv::var("HC_DATA").map(PathBuf::from) {
		if data_path.exists() {
			return Ok(data_path);
		}

		return Err(hc_error!(
			"data file {} (from HC_DATA) does not exist",
			data_path.display()
		));
	}

	if let Some(data_dir) = dirs::data_dir() {
		if data_dir.exists() {
			let data_path = pathbuf![&data_dir, "hipcheck"];
			if data_path.exists() {
				return Ok(data_path);
			}
		}
	}

	Err(hc_error!(
		"can't find data (try setting the `HC_DATA` environment variable or `--data <FILE>` flag)"
	))
}

/// Resolves the source specifier into an actual source.
fn resolve_source(
	source_type: &Check,
	phase: &mut Phase,
	home: &Path,
	source: &OsStr,
) -> Result<Source> {
	match source_type.check_type {
		CheckType::RepoSource => SourceRepo::resolve_repo(phase, home, source).map(|repo| Source {
			kind: SourceKind::Repo(repo),
		}),
		CheckType::PackageVersion => {
			let package = source.to_str().unwrap();

			let command = &source_type.to_owned().parent_command_value;

			let package_git_repo_url = pm::detect_and_extract(package, command.to_owned())
				.context("Could not get git repo URL for package")?;

			SourceRepo::resolve_repo(phase, home, OsStr::new(package_git_repo_url.as_str())).map(
				|repo| Source {
					kind: SourceKind::Repo(repo),
				},
			)
		}
		CheckType::PrUri => {
			SourceChangeRequest::resolve_change_request(phase, home, source).map(|cr| Source {
				kind: SourceKind::ChangeRequest(cr),
			})
		}
		CheckType::SpdxDocument => {
			let download_url = spdx::extract_download_url(source)?;
			SourceRepo::resolve_repo(phase, home, &download_url).map(|repo| Source {
				kind: SourceKind::Repo(repo),
			})
		}
		_ => Err(Error::msg("source specified was not a valid source")),
	}
}

pub struct Check {
	pub check_type: CheckType,
	pub check_value: OsString,

	//hc check 'parent_command_value', where parent_command_value request, repo, npm, maven, pypi etc
	pub parent_command_value: String,
	//pub check_url: Url, this does not seem to be used anywhere and was causing a ci error, so turning off
}

#[derive(Debug, PartialEq, Eq)]
pub enum CheckType {
	RepoSource,
	PrUri,
	PatchUri,
	PackageVersion,
	SpdxDocument,
}
#[cfg(test)]
mod tests {
	use super::*;
	use hc_common::test_util::with_env_vars;
	use tempfile::TempDir;

	const TEMPDIR_PREFIX: &str = "hc_test";

	#[test]
	fn resolve_token_test() {
		with_env_vars(vec![("HC_GITHUB_TOKEN", Some("test"))], || {
			let result = resolve_token().unwrap();
			println!("result token {}", result);
			let empty = "".to_string();
			assert_ne!(result, empty);
			assert_eq!(result, "test");
		});
	}

	#[test]
	fn resolve_home_with_home_env_var() {
		let tempdir = TempDir::with_prefix(TEMPDIR_PREFIX).unwrap();
		let tempdir_path = tempdir.path().to_string_lossy().into_owned();

		with_env_vars(
			vec![
				("HOME", Some(&tempdir_path)),
				("XDG_CACHE_HOME", None),
				("HC_HOME", None),
			],
			|| {
				let home_dir = None;
				let result = resolve_home(home_dir).unwrap();
				let path = result.to_str().unwrap();

				if cfg!(target_os = "linux") {
					let expected = pathbuf![&tempdir_path, ".cache", "hipcheck"];
					assert_eq!(path, expected.to_str().unwrap());
				} else if cfg!(target_os = "macos") {
					let expected = pathbuf![&tempdir_path, "Library", "Caches", "hipcheck"];
					assert_eq!(path, expected.to_str().unwrap());
				} else if cfg!(target_os = "windows") {
					let expected =
						pathbuf![&dirs::home_dir().unwrap(), "AppData", "Local", "hipcheck"];
					assert_eq!(path, expected.to_str().unwrap());
				} else {
					// Skip test if we cannot identify the OS
					let _path = path;
				}
			},
		);

		tempdir.close().unwrap();
	}

	#[test]
	fn resolve_home_with_home_flag() {
		let tempdir = TempDir::with_prefix(TEMPDIR_PREFIX).unwrap();
		let tempdir_path = tempdir.path().to_string_lossy().into_owned();

		with_env_vars(
			vec![
				("HOME", None),
				("XDG_CACHE_HOME", None),
				("HC_HOME", Some(&tempdir_path)),
			],
			|| {
				// Passing in config path that does not exist
				let manual_flag_path = &tempdir_path;
				let home_flag = pathbuf![manual_flag_path];
				let result = resolve_home(Some(&home_flag)).unwrap();
				let path = result.to_str().unwrap();

				if cfg!(target_os = "linux")
					|| cfg!(target_os = "macos")
					|| cfg!(target_os = "windows")
				{
					assert_eq!(path, manual_flag_path);
				} else {
					// Skip test if we cannot identify the OS
					let _path = path;
				}
			},
		);

		tempdir.close().unwrap();
	}

	#[test]
	fn resolve_home_with_xdg_cache_preferred() {
		let tempdir1 = TempDir::with_prefix(TEMPDIR_PREFIX).unwrap();
		let tempdir1_path = tempdir1.path().to_string_lossy().into_owned();

		let tempdir2 = TempDir::with_prefix(TEMPDIR_PREFIX).unwrap();
		let tempdir2_path = tempdir2.path().to_string_lossy().into_owned();

		with_env_vars(
			vec![
				("HOME", Some(&tempdir1_path)),
				("XDG_CACHE_HOME", Some(&tempdir2_path)),
				("HC_HOME", None),
			],
			|| {
				let result = resolve_home(None).unwrap();
				let path = result.to_str().unwrap();

				if cfg!(target_os = "linux") {
					let expected = pathbuf![&tempdir2_path, "hipcheck"];
					assert_eq!(path, expected.to_str().unwrap());
				} else if cfg!(target_os = "macos") {
					let expected = pathbuf![&tempdir1_path, "Library", "Caches", "hipcheck"];
					assert_eq!(path, expected.to_str().unwrap());
				} else if cfg!(target_os = "windows") {
					let expected =
						pathbuf![&dirs::home_dir().unwrap(), "AppData", "Local", "hipcheck"];
					assert_eq!(path, expected.to_str().unwrap());
				} else {
					// Skip test if we cannot identify the OS
					let _path = path;
				}
			},
		);

		tempdir1.close().unwrap();
		tempdir2.close().unwrap();
	}

	#[test]
	fn resolve_home_with_hc_home_preferred() {
		let tempdir = TempDir::with_prefix(TEMPDIR_PREFIX).unwrap();
		let tempdir_path = tempdir.path().to_string_lossy().into_owned();

		with_env_vars(
			vec![
				("HOME", Some("/users/foo")),
				("XDG_CACHE_HOME", Some("/xdg_cache_home")),
				("HC_HOME", Some(&tempdir_path)),
			],
			|| {
				let result = resolve_home(None).unwrap();
				let path = result.to_str().unwrap();
				// Skip test if we cannot identify the OS
				assert_eq!(path, &tempdir_path);
			},
		);

		tempdir.close().unwrap();
	}

	#[test]
	fn resolve_data_with_data_env_var() {
		let tempdir = TempDir::with_prefix(TEMPDIR_PREFIX).unwrap();
		let tempdir_path = tempdir.path().to_string_lossy().into_owned();

		with_env_vars(
			vec![
				("HOME", Some(&tempdir_path)),
				("XDG_DATA_HOME", None),
				("HC_DATA", None),
			],
			|| {
				let data_dir = None;
				let data_path = pathbuf![dirs::data_dir().unwrap(), "hipcheck"];
				create_dir_all(data_path.as_path()).unwrap();
				let result = resolve_data(data_dir).unwrap();
				let path = result.to_str().unwrap();

				if cfg!(target_os = "linux") {
					let expected = pathbuf![&tempdir_path, ".local", "share", "hipcheck"];
					assert_eq!(path, expected.to_str().unwrap());
				} else if cfg!(target_os = "macos") {
					let expected =
						pathbuf![&tempdir_path, "Library", "Application Support", "hipcheck"];
					assert_eq!(path, expected.to_str().unwrap());
				} else if cfg!(target_os = "windows") {
					let expected =
						pathbuf![&dirs::home_dir().unwrap(), "AppData", "Roaming", "hipcheck"];
					assert_eq!(path, expected.to_str().unwrap());
				} else {
					// Skip test if we cannot identify the OS
					let _path = path;
				}
			},
		);

		tempdir.close().unwrap();
	}

	#[test]
	fn resolve_data_with_data_flag() {
		let tempdir = TempDir::with_prefix(TEMPDIR_PREFIX).unwrap();
		let tempdir_path = tempdir.path().to_string_lossy().into_owned();

		with_env_vars(
			vec![
				("HOME", None),
				("XDG_DATA_HOME", None),
				("HC_DATA", Some(&tempdir_path)),
			],
			|| {
				// Passing in config path that does not exist
				let manual_flag_path = &tempdir_path;
				let data_flag = pathbuf![manual_flag_path];
				let result = resolve_data(Some(&data_flag)).unwrap();
				let path = result.to_str().unwrap();

				if cfg!(target_os = "linux")
					|| cfg!(target_os = "macos")
					|| cfg!(target_os = "windows")
				{
					assert_eq!(path, manual_flag_path);
				} else {
					// Skip test if we cannot identify the OS
					let _path = path;
				}
			},
		);

		tempdir.close().unwrap();
	}

	#[test]
	fn resolve_data_with_xdg_cache_preferred() {
		let tempdir1 = TempDir::with_prefix(TEMPDIR_PREFIX).unwrap();
		let tempdir1_path = tempdir1.path().to_string_lossy().into_owned();

		let tempdir2 = TempDir::with_prefix(TEMPDIR_PREFIX).unwrap();
		let tempdir2_path = tempdir2.path().to_string_lossy().into_owned();

		with_env_vars(
			vec![
				("HOME", Some(&tempdir1_path)),
				("XDG_DATA_HOME", Some(&tempdir2_path)),
				("HC_HOME", None),
				("HC_DATA", None),
			],
			|| {
				let data_path = pathbuf![dirs::data_dir().unwrap(), "hipcheck"];
				create_dir_all(data_path.as_path()).unwrap();
				let result = resolve_data(None).unwrap();
				let path = result.to_str().unwrap();

				if cfg!(target_os = "linux") {
					let expected = pathbuf![&tempdir2_path, "hipcheck"];
					assert_eq!(path, expected.to_str().unwrap());
				} else if cfg!(target_os = "macos") {
					let expected =
						pathbuf![&tempdir1_path, "Library", "Application Support", "hipcheck"];
					assert_eq!(path, expected.to_str().unwrap());
				} else if cfg!(target_os = "windows") {
					let expected =
						pathbuf![&dirs::home_dir().unwrap(), "AppData", "Roaming", "hipcheck"];
					assert_eq!(path, expected.to_str().unwrap());
				} else {
					// Skip test if we cannot identify the OS
					let _path = path;
				}
			},
		);

		tempdir1.close().unwrap();
		tempdir2.close().unwrap();
	}

	#[test]
	fn resolve_data_with_hc_data_preferred() {
		let tempdir = TempDir::with_prefix(TEMPDIR_PREFIX).unwrap();
		let tempdir_path = tempdir.path().to_string_lossy().into_owned();

		with_env_vars(
			vec![
				("HOME", Some("/users/foo")),
				("XDG_DATA_HOME", Some("/xdg_cache_home")),
				("HC_DATA", Some(&tempdir_path)),
			],
			|| {
				let result = resolve_data(None).unwrap();
				let path = result.to_str().unwrap();
				// This should work on all platforms
				assert_eq!(path, &tempdir_path);
			},
		);

		tempdir.close().unwrap();
	}
}
