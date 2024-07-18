use std::path::PathBuf;
use url::Url;

#[derive(Clone, Debug)]
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

#[derive(Clone, Debug)]
pub struct RemoteGitRepo {
	pub url: Url,
	pub known_remote: Option<KnownRemote>,
}

#[derive(Clone, Debug)]
pub enum KnownRemote {
	GitHub { owner: String, repo: String },
}

#[derive(Clone, Debug)]
pub struct LocalGitRepo {
	/// The path to the repo.
	pub path: PathBuf,

	/// The Git ref we're referring to.
	pub git_ref: String,
}

#[derive(Clone, Debug)]
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
#[derive(Clone, Debug)]
pub struct MavenPackage {
	/// The Maven url
	pub url: Url,
}

#[derive(Clone, Debug)]
// Maven as a possible host is ommitted because a MavenPackage is currently its own struct without a host field
pub enum PackageHost {
	Npm,
	PyPi,
}

#[derive(Clone, Debug)]
pub enum TargetSeed {
	LocalRepo(LocalGitRepo),
	RemoteRepo(RemoteGitRepo),
	Package(Package),
	MavenPackage(MavenPackage),
	Spdx(PathBuf),
}
impl ToString for TargetSeed {
	fn to_string(&self) -> String {
		todo!()
	}
}
