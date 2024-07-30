use crate::error::Error;
use std::fmt;
use std::fmt::Display;
use std::fmt::Formatter;
use std::path::PathBuf;
use url::Url;

#[derive(Clone, Debug, PartialEq, Eq)]
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteGitRepo {
	pub url: Url,
	pub known_remote: Option<KnownRemote>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KnownRemote {
	GitHub { owner: String, repo: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalGitRepo {
	/// The path to the repo.
	pub path: PathBuf,

	/// The Git ref we're referring to.
	pub git_ref: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MavenPackage {
	/// The Maven url
	pub url: Url,
}

#[derive(Clone, Debug, PartialEq, Eq)]
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sbom {
	/// The path to the SBOM file
	pub path: PathBuf,

	/// What standard the SBOM uses
	pub standard: SbomStandard,
}

#[derive(Clone, Debug, PartialEq, Eq)]
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
pub enum TargetSeed {
	LocalRepo(LocalGitRepo),
	RemoteRepo(RemoteGitRepo),
	Package(Package),
	MavenPackage(MavenPackage),
	Sbom(Sbom),
}

impl Display for TargetSeed {
	fn fmt(&self, f: &mut Formatter) -> fmt::Result {
		match self {
			TargetSeed::LocalRepo(repo) => write!(f, "local repo at {}", repo.path.display()),
			TargetSeed::RemoteRepo(remote_repo) => match &remote_repo.known_remote {
				Some(KnownRemote::GitHub { owner, repo }) => {
					write!(f, "GitHub repo {}/{} from {}", owner, repo, remote_repo.url)
				}
				_ => write!(f, "remote repo at {}", remote_repo.url.as_str()),
			},
			TargetSeed::Package(package) => write!(
				f,
				"{} package {}@{}",
				package.host, package.name, package.version
			),
			TargetSeed::MavenPackage(package) => {
				write!(f, "Maven package {}", package.url.as_str())
			}
			TargetSeed::Sbom(sbom) => {
				write!(f, "{} SBOM file at {}", sbom.standard, sbom.path.display())
			}
		}
	}
}

pub trait ToTargetSeed {
	fn to_target_seed(&self) -> Result<TargetSeed, Error>;
}
