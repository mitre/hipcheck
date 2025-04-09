// SPDX-License-Identifier: Apache-2.0

// NOTE: This file is shared as a build-dependency in `build.rs`! This will cause compilation errors when importing crates that are not build-dependencies.

use schemars::JsonSchema;
use serde::Serialize;
use std::{
	fmt,
	fmt::{Display, Formatter},
	path::PathBuf,
};
use url::Url;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Target {
	/// The original specifier provided by the user.
	pub specifier: String,

	/// The path to the local repository.
	pub local: LocalGitRepo,

	/// The url of the remote repository, if any.
	pub remote: Option<RemoteGitRepo>,

	/// The package associated with the target, if any.
	pub package: Option<Package>,
}

impl Target {
	/// git ref of the HEAD commit being analyzed
	pub fn head(&self) -> String {
		self.local.git_ref.clone()
	}

	/// gets the owner if there is a known Github remote repository
	pub fn owner(&self) -> Option<String> {
		// Gets the owner if there is a known GitHub remote repository
		let KnownRemote::GitHub { owner, repo: _ } =
			&self.remote.as_ref()?.known_remote.as_ref()?;
		Some(owner.clone())
	}

	/// name of the repository being analyzed
	pub fn name(&self) -> String {
		// In the future may want to augment Target/LocalGitRepo with a
		// "name" field. For now, treat the dir name of the repo as the name
		self.local
			.path
			.as_path()
			.file_name()
			.unwrap()
			.to_str()
			.unwrap()
			.to_owned()
	}
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct VcsUrl {
	pub remote: RemoteGitRepo,
	pub git_ref: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct RemoteGitRepo {
	pub url: Url,
	pub known_remote: Option<KnownRemote>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub enum KnownRemote {
	GitHub { owner: String, repo: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct LocalGitRepo {
	/// The path to the repo.
	pub path: PathBuf,

	/// The Git ref we're referring to.
	pub git_ref: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Package {
	/// A package url for the package.
	pub purl: Url,

	/// The package name
	pub name: String,

	/// The package version
	pub version: String,

	/// What host the package is from.
	pub host: PackageHost,
}
impl Package {
	pub fn has_version(&self) -> bool {
		self.version != Package::no_version()
	}
	pub fn no_version() -> &'static str {
		"no version"
	}
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct MavenPackage {
	/// The Maven url
	pub url: Url,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
// Maven as a possible host is ommitted because a MavenPackage is currently its own struct without a host field
pub enum PackageHost {
	Npm,
	PyPI,
}

impl Display for PackageHost {
	fn fmt(&self, f: &mut Formatter) -> fmt::Result {
		match self {
			PackageHost::Npm => write!(f, "Npm"),
			PackageHost::PyPI => write!(f, "PyPI"),
		}
	}
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Sbom {
	/// The path to the SBOM file
	pub path: PathBuf,

	/// What standard the SBOM uses
	pub standard: SbomStandard,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub enum SbomStandard {
	Spdx,
	CycloneDX,
}

impl Display for SbomStandard {
	fn fmt(&self, f: &mut Formatter) -> fmt::Result {
		match self {
			SbomStandard::Spdx => write!(f, "SPDX"),
			SbomStandard::CycloneDX => write!(f, "CycloneDX"),
		}
	}
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SingleTargetSeedKind {
	LocalRepo(LocalGitRepo),
	RemoteRepo(RemoteGitRepo),
	VcsUrl(VcsUrl),
	Package(Package),
	MavenPackage(MavenPackage),
	Sbom(Sbom),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SingleTargetSeed {
	pub kind: SingleTargetSeedKind,
	pub refspec: Option<String>,
	pub specifier: String,
}

#[allow(unused)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MultiTargetSeedKind {
	CargoToml(PathBuf),
	GoMod(PathBuf),
	PackageLockJson(PathBuf),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MultiTargetSeed {
	pub kind: MultiTargetSeedKind,
	pub specifier: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TargetSeedKind {
	Single(SingleTargetSeedKind),
	Multi(MultiTargetSeedKind),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TargetSeed {
	Single(SingleTargetSeed),
	Multi(MultiTargetSeed),
}

impl TargetSeed {
	/// Return whether the target seed is for multiple targets or not
	pub fn is_multi_target(&self) -> bool {
		match self {
			Self::Single(_) => false,
			Self::Multi(_) => true,
		}
	}
}

impl Display for SingleTargetSeedKind {
	fn fmt(&self, f: &mut Formatter) -> fmt::Result {
		use SingleTargetSeedKind::*;
		match self {
			LocalRepo(repo) => write!(f, "local repo at {}", repo.path.display()),
			RemoteRepo(remote) => match &remote.known_remote {
				Some(KnownRemote::GitHub { owner, repo }) => {
					write!(f, "GitHub repo {}/{} from {}", owner, repo, remote.url)
				}
				_ => write!(f, "remote repo at {}", remote.url.as_str()),
			},
			VcsUrl(vcs) => match &vcs.remote.known_remote {
				Some(KnownRemote::GitHub { owner, repo }) => {
					write!(f, "GitHub repo {}/{} from {}", owner, repo, vcs.remote.url)
				}
				_ => write!(f, "remote repo at {}", vcs.remote.url.as_str()),
			},
			Package(package) => {
				let ver_str = if package.has_version() {
					format!("@{}", package.version)
				} else {
					format!(" ({})", package.version)
				};
				write!(f, "{} package {}{}", package.host, package.name, ver_str)
			}
			MavenPackage(package) => {
				write!(f, "Maven package {}", package.url.as_str())
			}
			Sbom(sbom) => {
				write!(f, "{} SBOM file at {}", sbom.standard, sbom.path.display())
			}
		}
	}
}

impl Display for TargetSeed {
	fn fmt(&self, f: &mut Formatter) -> fmt::Result {
		match self {
			TargetSeed::Single(x) => x.kind.fmt(f),
			TargetSeed::Multi(_x) => unimplemented!(),
		}
	}
}
