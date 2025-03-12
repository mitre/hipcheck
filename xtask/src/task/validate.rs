// SPDX-License-Identifier: Apache-2.0

//! Validate the configuration of all Hipcheck crates.

use crate::workspace;
use anyhow::{anyhow, Context as _, Result};
use glob::glob;
use pathbuf::pathbuf;
use pep440_rs::{Version, VersionSpecifiers};
use pyproject_toml::{Contact, License, Project, PyProjectToml};
use serde::{de::DeserializeOwned, Deserialize};
use std::{
	collections::BTreeSet,
	fmt::{self, Debug, Display, Formatter},
	fs::{self, File},
	io::{BufRead, BufReader},
	ops::Not as _,
	path::{Path, PathBuf},
	str::FromStr,
};

/// Print list of validation failures for packages in the workspace.
pub fn run() -> Result<()> {
	log::info!("beginning validation");

	let workspace = Workspace::resolve()?;
	let findings = Findings::for_workspace(&workspace);
	findings.report()?;

	if findings.package_findings.is_empty()
		&& findings.config_findings.is_empty()
		&& findings.source_findings.is_empty()
	{
		log::info!("all checks passed!");
	} else {
		log::info!("not all checks passed");
	}
	Ok(())
}

/// Set of package findings.
type PackageFindingsSet = BTreeSet<PackageIssues>;

/// Vector (rather than HashMap) mapping packages to findings.
type PackageFindings<'work> = Vec<(&'work Package, PackageFindingsSet)>;

/// Set of config findings.
type ConfigFindingsSet = BTreeSet<ConfigIssues>;

/// Vector (rather than HashMap) mapping config files to findings.
type ConfigFindings<'work> = Vec<(&'work Path, ConfigFindingsSet)>;

/// Set of source findings.
type SourceFindingsSet = Vec<(PathBuf, SourceIssues)>;

/// Vector (rather than HashMap) mapping packages to source findings.
type SourceFindings<'work> = Vec<(&'work Package, SourceFindingsSet)>;

/// Maps packages to findings.
///
/// Retains a reference to the overall workspace because it's needed when printing results.
struct Findings<'work> {
	/// Reference to the workspace (kept for printing results)
	workspace: &'work Workspace,
	/// The findings for each package.
	package_findings: PackageFindings<'work>,
	/// Findings for the Hipcheck configuration files.
	config_findings: ConfigFindings<'work>,
	/// Findings for Rust source files.
	source_findings: SourceFindings<'work>,
}

impl<'work> Findings<'work> {
	/// Perform validation of packages in the workspace.
	fn for_workspace(workspace: &'work Workspace) -> Findings<'work> {
		let package_findings: PackageFindings<'work> = workspace
			.packages
			.iter()
			.fold(Vec::new(), |mut package_findings, package| {
				package_findings.push((package, validate_package(package)));
				package_findings
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
			.packages
			.iter()
			.fold(Vec::new(), |mut source_findings, package| {
				source_findings.push((package, validate_sources(package)));
				source_findings
			})
			.into_iter()
			.filter(|(_, findings)| findings.is_empty().not())
			.collect();

		Findings {
			workspace,
			package_findings,
			config_findings,
			source_findings,
		}
	}

	/// Report the findings.
	fn report(&self) -> Result<()> {
		for (package, findings) in &self.package_findings {
			let package_path = package.path.strip_prefix(&self.workspace.root)?;
			let package_language = match package.config {
				PackageConfig::Crate(_) => "Rust",
				PackageConfig::PyProject(_) => "Python",
			};

			for finding in findings.iter() {
				log::error!(
					"package: {}, package_path: {}, package_language: {}, name: {:?}, desc: {}",
					package.name,
					package_path.display(),
					package_language,
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

		for (package, findings) in &self.source_findings {
			let package_path = package.path.strip_prefix(&self.workspace.root)?;
			let package_language = match package.config {
				PackageConfig::Crate(_) => "Rust",
				PackageConfig::PyProject(_) => "Python",
			};
			for (file, finding) in findings.iter() {
				let source_path = file.strip_prefix(&self.workspace.root)?;
				log::error!(
					"package: {}, package_path: {}, package_language: {}, source: {}, name: {:?}, desc: {}",
					package.name,
					package_path.display(),
					package_language,
					source_path.display(),
					finding,
					finding
				);
			}
		}

		Ok(())
	}
}

/// Types of issues packages can have.
#[derive(Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum PackageIssues {
	/// The package has authors when it shouldn't.
	HasAuthors,
	/// The package has no authors when it should.
	MissingAuthors,
	/// The crate license is present.
	LicensePresent,
	/// The python project is missing license that duplicates the workspace license
	NoDuplicateLicense,
	/// The license config in the manifest is invalid.
	LicenseInvalid,
	/// Crate is using an edition other than 2021.
	Not2021Edition,
	/// Python project does not support Python version 3.10
	NotPython3_10,
}

impl Display for PackageIssues {
	fn fmt(&self, f: &mut Formatter) -> fmt::Result {
		use PackageIssues::*;

		let msg = match self {
			HasAuthors => "must not have an authors field in the 'Cargo.toml' file",
			MissingAuthors => "must have an authors filed in the 'pyproject.toml' file",
			LicensePresent => "must not have a 'LICENSE.md' file",
			NoDuplicateLicense => {
				"must have a single license file that matches the Hipcheck license file"
			}
			LicenseInvalid => "license must be set to `'Apache-2.0'`",
			Not2021Edition => "edition must be set to '2021' in 'Cargo.toml'",
			NotPython3_10 => "python projects must support Python version 3.10",
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

/// Perform validation of a package, producing findings.
fn validate_package(package: &Package) -> PackageFindingsSet {
	match &package.config {
		PackageConfig::Crate(_) => validate_crate(package),
		PackageConfig::PyProject(_) => validate_pyproject(package),
	}
}

/// Perform validation of a Rust crate manifest, producing findings.
fn validate_crate(krate: &Package) -> PackageFindingsSet {
	use PackageIssues::*;

	// Panic: We only call this function if the package is a Cargo crate
	let PackageConfig::Crate(manifest) = &krate.config else {
		panic!()
	};

	let mut findings = BTreeSet::new();

	log::info!("validating crate '{}' doesn't specify authors", krate.name);
	if crate_has_authors(manifest) {
		findings.insert(HasAuthors);
	}

	log::info!(
		"validating crate '{}' doesn't specify license_file",
		krate.name
	);
	if crate_license_file_present(krate) {
		findings.insert(PackageIssues::LicensePresent);
	}

	log::info!(
		"validating crate '{}' specifies the correct license",
		krate.name
	);
	if crate_license_invalid(manifest) {
		findings.insert(LicenseInvalid);
	}

	log::info!("validating crate '{}' uses the correct edition", krate.name);
	if crate_uses_wrong_edition(manifest) {
		findings.insert(Not2021Edition);
	}

	findings
}

/// Check if the 'Cargo.toml' file has an authors field.
fn crate_has_authors(manifest: &CrateManifest) -> bool {
	manifest.package.authors.is_some()
}

/// Check if the crate has a 'LICENSE.md' file.
fn crate_license_file_present(krate: &Package) -> bool {
	// Panic: For a Rust crate, there will always be a single license path
	krate.license_paths.as_ref().unwrap()[0].exists()
}

/// Check if the crate license isn't the expected one.
fn crate_license_invalid(manifest: &CrateManifest) -> bool {
	match &manifest.package.license {
		Some(license) => license.as_str() != "Apache-2.0",
		None => true,
	}
}

/// Check if the `Cargo.toml` file specifies the wrong edition of Rust
fn crate_uses_wrong_edition(manifest: &CrateManifest) -> bool {
	manifest
		.package
		.edition
		.as_ref()
		.map(|e| e != "2021")
		.unwrap_or(true)
}

/// Perform validation of a python project, producing findings.
fn validate_pyproject(pyproject: &Package) -> PackageFindingsSet {
	use PackageIssues::*;

	let mut findings = BTreeSet::new();

	// Panic: We only call this function if the package is a Python project
	let PackageConfig::PyProject(py_config) = &pyproject.config else {
		panic!()
	};

	log::info!(
		"validating Python project '{}' specifies authors",
		pyproject.name
	);
	if pyproject_missing_authors(py_config) {
		findings.insert(MissingAuthors);
	}

	log::info!(
		"validating Python project '{}' doesn't specify license_file",
		pyproject.name
	);
	if pyproject_license_file_missing(pyproject) {
		findings.insert(PackageIssues::NoDuplicateLicense);
	}

	log::info!(
		"validating Python project '{}' specifies the correct license",
		pyproject.name
	);
	if pyproject_license_invalid(py_config) {
		findings.insert(LicenseInvalid);
	}

	log::info!(
		"validating Python project '{}' uses the correct version",
		pyproject.name
	);
	if pyproject_uses_wrong_version(py_config) {
		findings.insert(NotPython3_10);
	}

	findings
}

/// Check if the 'pyproject.toml' file has an authors field.
fn pyproject_missing_authors(py_config: &PyProjectConfiguration) -> bool {
	py_config.authors.is_none()
}

/// Check if the Python project is missing a 'LICENSE.md' file or has one that is not identical to the Hipcheck license.
fn pyproject_license_file_missing(pyproject: &Package) -> bool {
	// Check that license paths were specified in the pyproject.toml
	let Some(license_paths) = &pyproject.license_paths else {
		return true;
	};
	// Check that only one license path was specified
	if license_paths.len() != 1 {
		return true;
	}

	// Check that the Python project license file exists and read its contents if it does
	let license_path = &license_paths[0];
	let Ok(python_license) = fs::read(license_path) else {
		return true;
	};

	// Read the workspace license file

	// Panic: Safe to unwrap because we have already called this function without error when finding the license path
	let root = workspace::root().unwrap();
	let hipcheck_license_path = pathbuf![&root, "LICENSE"];
	// Panic: The Hipcheck 'LICENSE' file should exist and be readable
	let hipcheck_license =
		fs::read(hipcheck_license_path).expect("Unable to read Hipcheck license file");

	// Compare the license files
	python_license != hipcheck_license
}

/// Check if the Python project license isn't the expected one.
fn pyproject_license_invalid(py_config: &PyProjectConfiguration) -> bool {
	match &py_config.license {
		Some(License::Spdx(license)) => license != "Apache-2.0",
		// Currently we do not check for unexpected licenses if the license information is anything other than an SPDX string
		_ => true,
	}
}

/// Check if the 'pyproject.toml' file does not indicate support for the correct version of Python
fn pyproject_uses_wrong_version(py_config: &PyProjectConfiguration) -> bool {
	let version = Version::from_str("3.10").unwrap();

	match &py_config.version_specs {
		Some(version_specs) => !version_specs.contains(&version),
		None => true,
	}
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

fn validate_sources(package: &Package) -> SourceFindingsSet {
	let mut findings = Vec::new();

	// Get paths to *.rs files
	let rust_pattern = format!("{}/**/*.rs", package.path.display());
	// PANIC SAFETY: We should always be able to parse the globbing pattern.
	let rust_globber = glob(&rust_pattern).expect("failed to parse globbing pattern");

	// Get paths to *.py files
	let py_pattern = format!("{}/**/*.py", package.path.display());
	// PANIC SAFETY: We should always be able to parse the globbing pattern.
	let py_globber = glob(&py_pattern).expect("failed to parse globbing pattern");

	// Validate files of either extension
	for path_result in rust_globber.chain(py_globber) {
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
		Some(Ok(line)) => {
			str::trim(&line) != "// SPDX-License-Identifier: Apache-2.0"
				&& str::trim(&line) != "# SPDX-License-Identifier: Apache-2.0"
		}
		// Treat any inability to read a line or the lack of lines as an indicator that the
		// license comment is missing.
		_ => true,
	}
}

/// Owns all the crates in the workspace.
#[derive(Debug)]
struct Workspace {
	/// The packages in the workspace
	packages: Vec<Package>,
	/// The root path of the workspace.
	root: PathBuf,
	/// Paths to configuration files.
	configs: Vec<PathBuf>,
}

impl Workspace {
	/// Figure out what's in the workspace.
	fn resolve() -> Result<Workspace> {
		let root = workspace::root()?;

		// Get Rust crates from workspace Cargo.toml
		let mut packages = {
			let manifest_path = pathbuf![&root, "Cargo.toml"];
			read_toml::<&Path, WorkspaceManifest>(&manifest_path)?.crates(&root)?
		};

		// Manually add our single Python project
		let python_sdk_path = pathbuf![&root, "sdk", "python"];
		let python_sdk = Package::python_at_path(python_sdk_path)?;

		let mut pyprojects = vec![python_sdk];
		packages.append(&mut pyprojects);

		let configs = {
			let config_dir = pathbuf![&root, "config"];
			resolve_configs(&config_dir)?
		};

		Ok(Workspace {
			packages,
			root,
			configs,
		})
	}
}

/// A single package (here meaning a "unit of reuse"), such as a Rust crate
#[derive(Debug)]
pub struct Package {
	/// The name of the package (like "hc_core")
	name: String,
	/// The path to the package
	path: PathBuf,
	/// Data from the package configuration file (e.g. 'Cargo.toml', 'pyproject.toml')
	config: PackageConfig,
	/// The paths to any license files, which may or may not be present.
	license_paths: Option<Vec<PathBuf>>,
}

impl Package {
	/// Identify information for the Rust crate at the given path.
	fn crate_at_path(path: PathBuf) -> Result<Package> {
		let name = path
			.file_name()
			.ok_or_else(|| anyhow!("missing crate name"))?
			.to_string_lossy()
			.into_owned();

		let manifest = {
			let manifest_path = pathbuf![&path, "Cargo.toml"];
			read_toml::<&Path, CrateManifest>(&manifest_path)?
		};
		let config = PackageConfig::Crate(manifest);

		let license_paths = Some(vec![pathbuf![&path, "LICENSE.md"]]);

		Ok(Package {
			name,
			path,
			config,
			license_paths,
		})
	}

	/// Identify information for the Python project at the given path
	fn python_at_path(path: PathBuf) -> Result<Package> {
		let name = path
			.file_name()
			.ok_or_else(|| anyhow!("missing crate name"))?
			.to_string_lossy()
			.into_owned();

		let project = {
			let pyproject_path = pathbuf![&path, "pyproject.toml"];
			read_pyproject_toml::<&Path>(&pyproject_path)?
		};

		let authors = project.authors;
		let license = project.license;
		let version_specs = project.requires_python;

		let py_config = PyProjectConfiguration {
			authors,
			license,
			version_specs,
		};
		let config = PackageConfig::PyProject(py_config);

		let license_files = project.license_files;
		let license_paths =
			license_files.map(|l| l.into_iter().map(|f| pathbuf![&path, &f]).collect());

		Ok(Package {
			name,
			path,
			config,
			license_paths,
		})
	}
}

/// Data from a package configuration file
#[derive(Debug)]
enum PackageConfig {
	Crate(CrateManifest),
	PyProject(PyProjectConfiguration),
}

/// The direct representation of the top-level 'Cargo.toml' file.
#[derive(Debug, Deserialize)]
struct WorkspaceManifest {
	/// The workspace section of that file.
	workspace: WorkspaceManifestInner,
}

impl WorkspaceManifest {
	/// Get the crates defined by the workspace manifest.
	fn crates(&self, root: &Path) -> Result<Vec<Package>> {
		let mut entries = Vec::new();

		for member in &self.workspace.members {
			// Get the full string to glob.
			let to_glob = pathbuf![root, member].display().to_string();

			// For each path it expands to, fill out the crate info.
			for path in glob(&to_glob)? {
				entries.push(Package::crate_at_path(path?)?);
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

/// The configuration file for individual python projects.
#[derive(Debug, Deserialize)]
struct PyProjectConfiguration {
	/// The author information.
	authors: Option<Vec<Contact>>,
	/// The license of the project.
	license: Option<License>,
	/// The version speicifiers of Python that the project requires.
	version_specs: Option<VersionSpecifiers>,
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

/// Read file to a struct that can be deserialized from pyproject.toml format.
fn read_pyproject_toml<P: AsRef<Path>>(path: P) -> Result<Project> {
	let path = path.as_ref();
	let contents = read_string(path)?;
	let pyproject = PyProjectToml::new(&contents)
		.with_context(|| format!("failed to read as pyproject.toml '{}'", path.display()))?;
	pyproject.project.ok_or(anyhow!(
		"pyproject.toml missing project information '{}'",
		path.display()
	))
}

/// Read a file to a string.
fn read_string<P: AsRef<Path>>(path: P) -> Result<String> {
	fn inner(path: &Path) -> Result<String> {
		fs::read_to_string(path)
			.with_context(|| format!("failed to read as UTF-8 string '{}'", path.display()))
	}

	inner(path.as_ref())
}
