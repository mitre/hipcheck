// SPDX-License-Identifier: Apache-2.0

//! Validate the configuration of all Hipcheck crates.

use crate::workspace;
use anyhow::anyhow;
use anyhow::Context as _;
use anyhow::Result;
use glob::glob;
use pathbuf::pathbuf;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::collections::BTreeSet;
use std::fmt;
use std::fmt::Debug;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fs;
use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
use std::ops::Not as _;
use std::path::Path;
use std::path::PathBuf;

/// Print list of validation failures for crates in the workspace.
pub fn run() -> Result<()> {
	log::info!("beginning validation");

	let workspace = Workspace::resolve()?;
	let findings = Findings::for_workspace(&workspace);
	findings.report()?;

	log::info!("all checks passed!");
	Ok(())
}

/// Set of crate findings.
type CrateFindingsSet = BTreeSet<CrateIssues>;

/// Vector (rather than HashMap) mapping crates to findings.
type CrateFindings<'work> = Vec<(&'work Crate, CrateFindingsSet)>;

/// Set of config findings.
type ConfigFindingsSet = BTreeSet<ConfigIssues>;

/// Vector (rather than HashMap) mapping config files to findings.
type ConfigFindings<'work> = Vec<(&'work Path, ConfigFindingsSet)>;

/// Set of source findings.
type SourceFindingsSet = Vec<(PathBuf, SourceIssues)>;

/// Vector (rather than HashMap) mapping config files to findings.
type SourceFindings<'work> = Vec<(&'work Crate, SourceFindingsSet)>;

/// Maps crates to findings.
///
/// Retains a reference to the overall workspace because it's needed when printing results.
struct Findings<'work> {
	/// Reference to the workspace (kept for printing results)
	workspace: &'work Workspace,
	/// The findings for each crate.
	crate_findings: CrateFindings<'work>,
	/// Findings for the Hipcheck configuration files.
	config_findings: ConfigFindings<'work>,
	/// Findings for source files.
	source_findings: SourceFindings<'work>,
}

impl<'work> Findings<'work> {
	/// Perform validation of crates in the workspace.
	fn for_workspace(workspace: &'work Workspace) -> Findings<'work> {
		let crate_findings: CrateFindings<'work> = workspace
			.crates
			.iter()
			.fold(Vec::new(), |mut crate_findings, krate| {
				crate_findings.push((krate, validate_crate(krate)));
				crate_findings
			})
			.into_iter()
			.filter(|(_, findings)| findings.is_empty().not())
			.collect();

		let config_findings: ConfigFindings<'work> = workspace
			.configs
			.iter()
			.fold(Vec::new(), |mut config_findings, config| {
				config_findings.push((config.as_ref(), validate_config(config)));
				config_findings
			})
			.into_iter()
			.filter(|(_, findings)| findings.is_empty().not())
			.collect();

		let source_findings: SourceFindings<'work> = workspace
			.crates
			.iter()
			.fold(Vec::new(), |mut source_findings, krate| {
				source_findings.push((krate, validate_sources(krate)));
				source_findings
			})
			.into_iter()
			.filter(|(_, findings)| findings.is_empty().not())
			.collect();

		Findings {
			workspace,
			crate_findings,
			config_findings,
			source_findings,
		}
	}

	/// Report the findings.
	fn report(&self) -> Result<()> {
		for (krate, findings) in &self.crate_findings {
			let krate_path = krate.path.strip_prefix(&self.workspace.root)?;
			for finding in findings.iter() {
				log::error!(
					"crate: {}, crate_path: {}, name: {:?}, desc: {}",
					krate.name,
					krate_path.display(),
					finding,
					finding
				);
			}
		}

		for (config, findings) in &self.config_findings {
			let config_path = config.strip_prefix(&self.workspace.root)?;

			for finding in findings.iter() {
				log::error!(
					"config_path: {}, name: {:?}, desc: {}",
					config_path.display(),
					finding,
					finding
				);
			}
		}

		for (krate, findings) in &self.source_findings {
			let krate_path = krate.path.strip_prefix(&self.workspace.root)?;
			for (file, finding) in findings.iter() {
				let source_path = file.strip_prefix(&self.workspace.root)?;
				log::error!(
					"crate: {}, crate_path: {}, source: {}, name: {:?}, desc: {}",
					krate.name,
					krate_path.display(),
					source_path.display(),
					finding,
					finding
				);
			}
		}

		Ok(())
	}
}

/// Types of issues crates can have.
#[derive(Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum CrateIssues {
	/// The crate has authors when it shouldn't.
	HasAuthors,
	/// The crate license is present.
	LicensePresent,
	/// The license config in the manifest is invalid.
	LicenseInvalid,
	/// Crate is using an edition other than 2021.
	Not2021Edition,
}

impl Display for CrateIssues {
	fn fmt(&self, f: &mut Formatter) -> fmt::Result {
		use CrateIssues::*;

		let msg = match self {
			HasAuthors => "must not have an authors field in 'Cargo.toml'",
			LicensePresent => "must not have a 'LICENSE.md' file",
			LicenseInvalid => "license must be set to `'Apache-2.0'`",
			Not2021Edition => "edition must be set to '2021' in 'Cargo.toml'",
		};

		write!(f, "{}", msg)
	}
}

#[derive(PartialEq, Eq, Hash, PartialOrd, Ord)]
enum ConfigIssues {
	InvalidSyntax(String),
}

impl Debug for ConfigIssues {
	fn fmt(&self, f: &mut Formatter) -> fmt::Result {
		use ConfigIssues::*;

		match self {
			InvalidSyntax(_) => write!(f, "InvalidSyntax"),
		}
	}
}

impl Display for ConfigIssues {
	fn fmt(&self, f: &mut Formatter) -> fmt::Result {
		use ConfigIssues::*;

		match self {
			InvalidSyntax(msg) => write!(f, "{}", msg),
		}
	}
}

/// Perform validation of a crate, producing findings.
fn validate_crate(krate: &Crate) -> CrateFindingsSet {
	use CrateIssues::*;

	let mut findings = BTreeSet::new();

	log::info!("validating crate '{}' doesn't specify authors", krate.name);
	if crate_has_authors(krate) {
		findings.insert(HasAuthors);
	}

	log::info!(
		"validating crate '{}' doesn't specify license_file",
		krate.name
	);
	if crate_license_file_present(krate) {
		findings.insert(LicensePresent);
	}

	log::info!(
		"validating crate '{}' specifies the correct license",
		krate.name
	);
	if crate_license_invalid(krate) {
		findings.insert(LicenseInvalid);
	}

	log::info!("validating crate '{}' uses the correct edition", krate.name);
	if crate_uses_wrong_edition(krate) {
		findings.insert(Not2021Edition);
	}

	findings
}

/// Check if the 'Cargo.toml' file has an authors field.
fn crate_has_authors(krate: &Crate) -> bool {
	krate.manifest.package.authors.is_some()
}

/// Check if the crate has a 'LICENSE.md' file.
fn crate_license_file_present(krate: &Crate) -> bool {
	krate.license_path.exists()
}

/// Check if the crate license isn't the expected one.
fn crate_license_invalid(krate: &Crate) -> bool {
	match &krate.manifest.package.license {
		Some(license) => license.as_str() != "Apache-2.0",
		None => true,
	}
}

fn crate_uses_wrong_edition(krate: &Crate) -> bool {
	krate
		.manifest
		.package
		.edition
		.as_ref()
		.map(|e| e != "2021")
		.unwrap_or(true)
}

/// Perform validation of a configuration file, producing findings.
fn validate_config(config: &Path) -> ConfigFindingsSet {
	use ConfigIssues::*;

	let mut findings = BTreeSet::new();

	log::info!("validating config file '{}' syntax", config.display());
	if let Err(msg) = config_syntax_invalid(config) {
		findings.insert(InvalidSyntax(msg));
	}

	findings
}

/// Check if the configuration file fails to deserialize.
fn config_syntax_invalid(config: &Path) -> std::result::Result<(), String> {
	read_toml::<&Path, toml::Value>(config)
		.map(|_| ())
		.map_err(|err| err.root_cause().to_string())
}

#[derive(PartialEq, Eq, Hash, PartialOrd, Ord)]
enum SourceIssues {
	MissingLicenseComment,
}

impl Debug for SourceIssues {
	fn fmt(&self, f: &mut Formatter) -> fmt::Result {
		use SourceIssues::*;

		match self {
			MissingLicenseComment => write!(f, "MissingLicenseComment"),
		}
	}
}

impl Display for SourceIssues {
	fn fmt(&self, f: &mut Formatter) -> fmt::Result {
		use SourceIssues::*;

		match self {
			MissingLicenseComment => write!(f, "missing SPDX license comment at beginning of file"),
		}
	}
}

/// Perform validation of the source files of a crate.
fn validate_sources(krate: &Crate) -> SourceFindingsSet {
	let pattern = format!("{}/**/*.rs", krate.path.display());
	// PANIC SAFETY: We should always be able to parse the globbing pattern.
	let globber = glob(&pattern).expect("failed to parse globbing pattern");

	let mut findings = Vec::new();

	for path_result in globber {
		match path_result {
			Ok(path) => {
				log::info!(
					"validating source file '{}' includes an SPDX license comment",
					path.display()
				);
				if source_missing_license_comment(&path) {
					findings.push((path, SourceIssues::MissingLicenseComment));
				}
			}
			Err(e) => {
				log::warn!(
					"warn: failed to check glob against path: {}",
					e.path().display()
				);
			}
		}
	}

	findings
}

/// Check if a source file has the expected license comment at the beginning.
fn source_missing_license_comment(source_path: &Path) -> bool {
	// Treat any inability to read the file as an indicator that the license comment is missing.
	let file = match File::open(source_path) {
		Ok(f) => f,
		Err(_) => return true,
	};

	let reader = BufReader::new(file);

	match reader.lines().next() {
		Some(Ok(line)) => str::trim(&line) != "// SPDX-License-Identifier: Apache-2.0",
		// Treat any inability to read a line or the lack of lines as an indicator that the
		// license comment is missing.
		_ => true,
	}
}

/// Owns all the crates in the workspace.
#[derive(Debug)]
struct Workspace {
	/// The crates in the workspace.
	crates: Vec<Crate>,
	/// The root path of the workspace.
	root: PathBuf,
	/// Paths to configuration files.
	configs: Vec<PathBuf>,
}

impl Workspace {
	/// Figure out what's in the workspace.
	fn resolve() -> Result<Workspace> {
		let root = workspace::root()?;

		let crates = {
			let manifest_path = pathbuf![&root, "Cargo.toml"];
			read_toml::<&Path, WorkspaceManifest>(&manifest_path)?.crates(&root)?
		};

		let configs = {
			let config_dir = pathbuf![&root, "config"];
			resolve_configs(&config_dir)?
		};

		Ok(Workspace {
			crates,
			root,
			configs,
		})
	}
}

/// A single crate.
#[derive(Debug)]
pub struct Crate {
	/// The name of the crate (like "hc_core")
	name: String,
	/// The path to the crate
	path: PathBuf,
	/// Data from the crate manifest ('Cargo.toml')
	manifest: CrateManifest,
	/// The path to the license file, which may or may not be present.
	license_path: PathBuf,
}

impl Crate {
	/// Identify information for the crate at the given path.
	fn at_path(path: PathBuf) -> Result<Crate> {
		let name = path
			.file_name()
			.ok_or_else(|| anyhow!("missing crate name"))?
			.to_string_lossy()
			.into_owned();

		let manifest = {
			let manifest_path = pathbuf![&path, "Cargo.toml"];
			read_toml::<&Path, CrateManifest>(&manifest_path)?
		};

		let license_path = pathbuf![&path, "LICENSE.md"];

		Ok(Crate {
			name,
			path,
			manifest,
			license_path,
		})
	}
}

/// The direct representation of the top-level 'Cargo.toml' file.
#[derive(Debug, Deserialize)]
struct WorkspaceManifest {
	/// The workspace section of that file.
	workspace: WorkspaceManifestInner,
}

impl WorkspaceManifest {
	/// Get the crates defined by the workspace manifest.
	fn crates(&self, root: &Path) -> Result<Vec<Crate>> {
		let mut entries = Vec::new();

		for member in &self.workspace.members {
			// Get the full string to glob.
			let to_glob = pathbuf![root, member].display().to_string();

			// For each path it expands to, fill out the crate info.
			for path in glob(&to_glob)? {
				entries.push(Crate::at_path(path?)?);
			}
		}

		Ok(entries)
	}
}

/// The workspace section of the top-level manifest file.
#[derive(Debug, Deserialize)]
struct WorkspaceManifestInner {
	/// The list of members (as globs)
	members: Vec<String>,
}

/// The manifest file for individual crates.
#[derive(Debug, Deserialize)]
struct CrateManifest {
	/// The package section.
	package: CrateManifestPackage,
}

/// The package section of the manifest file for individual crates.
#[derive(Debug, Deserialize)]
struct CrateManifestPackage {
	/// The author information.
	authors: Option<Vec<String>>,
	/// The license of the crate.
	license: Option<String>,
	/// The edition of Rust that the crate uses.
	edition: Option<String>,
}

/// Identify all the Hipcheck configuration files in the workspace.
fn resolve_configs(dir: &Path) -> Result<Vec<PathBuf>> {
	let to_glob = format!("{}/*.toml", dir.display());

	let mut configs = Vec::new();

	for config in glob(&to_glob)? {
		configs.push(config?);
	}

	Ok(configs)
}

/// Read file to a struct that can be deserialized from TOML format.
fn read_toml<P: AsRef<Path>, T: DeserializeOwned>(path: P) -> Result<T> {
	let path = path.as_ref();
	let contents = read_string(path)?;
	toml::de::from_str(&contents)
		.with_context(|| format!("failed to read as TOML '{}'", path.display()))
}

/// Read a file to a string.
fn read_string<P: AsRef<Path>>(path: P) -> Result<String> {
	fn inner(path: &Path) -> Result<String> {
		fs::read_to_string(path)
			.with_context(|| format!("failed to read as UTF-8 string '{}'", path.display()))
	}

	inner(path.as_ref())
}
