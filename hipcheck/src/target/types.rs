use std::path::PathBuf;
use url::Url;

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

pub struct RemoteGitRepo {
	url: Url,
	known_remote: Option<KnownRemote>,
}

pub enum KnownRemote {
	GitHub { owner: String, repo: String },
}

pub struct LocalGitRepo {
	/// The path to the repo.
	path: PathBuf,

	/// The Git ref we're referring to.
	git_ref: String,
}

pub struct Package {
	/// A package url for the package.
	purl: String,

	/// The package name
	name: String,

	/// The package version
	version: String,

	/// What host the package is from.
	host: PackageHost,
}

pub enum PackageHost {
	Npm,
	Maven,
	PyPi,
}
