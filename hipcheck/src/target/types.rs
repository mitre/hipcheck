use std::path::PathBuf;
use url::Url;

#[derive(Clone, Debug)]
pub struct Target {
	/// The original specifier provided by the user.
	specifier: String,

	/// The path to the local repository.
	local: LocalGitRepo,

	/// The url of the remote repository, if any.
	remote: Option<RemoteGitRepo>,

	/// The package associated with the target, if any.
	package: Option<Package>,
}

#[derive(Clone, Debug)]
pub struct RemoteGitRepo {
	url: Url,
	known_remote: Option<KnownRemote>,
}

#[derive(Clone, Debug)]
pub enum KnownRemote {
	GitHub { owner: String, repo: String },
}

#[derive(Clone, Debug)]
pub struct LocalGitRepo {
	/// The path to the repo.
	path: PathBuf,

	/// The Git ref we're referring to.
	git_ref: String,
}

#[derive(Clone, Debug)]
pub struct Package {
	/// A package url for the package.
	purl: Url,

	/// The package name
	name: String,

	/// The package version
	version: String,

	/// What host the package is from.
	host: PackageHost,
}

#[derive(Clone, Debug)]
pub enum PackageHost {
	Npm,
	Maven,
	PyPi,
}

#[derive(Clone, Debug)]
pub enum TargetSeed {
	LocalRepo(LocalGitRepo),
	RemoteRepo(RemoteGitRepo),
	Package(Package),
	Spdx(PathBuf),
}
impl ToString for TargetSeed {
	fn to_string(&self) -> String {
		todo!()
	}
}
