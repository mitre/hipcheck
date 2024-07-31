// SPDX-License-Identifier: Apache-2.0

use super::git;
use crate::context::Context;
use crate::data::git_command::GitCommand;
use crate::error::Error;
use crate::error::Result;
use crate::hc_error;
use crate::shell::spinner_phase::SpinnerPhase;
pub use crate::source::query::*;
use crate::target::{KnownRemote, LocalGitRepo, RemoteGitRepo, Target};
use log::debug;
use pathbuf::pathbuf;
use std::path::Path;
use std::path::PathBuf;
use url::Host;
use url::Url;

/// Resolving is how we ensure we have a valid, ready-to-go source of Git data
/// for the rest of Hipcheck's analysis. The below functions handle the resolution
/// of local or remote repos.
///
/// If the repo is local, the resolve function will work with the local repository
/// without cloning (all operationsare write-only, so this won't harm the repo at
/// all).
///
/// If it's a remote source, Hipcheck will clone the source so it can work with a
/// local copy, putting the clone in '<root>/clones'. It also notes whether a
/// remote repo is from a known or unknown host, because some forms of analysis
/// rely on accessing the API's of certain known hosts (currently just GitHub).
///
/// In either case, it also gets the commit head of the HEAD commit, so we can
/// make sure future operations are all done relative to the HEAD, and that any
/// cached data records what the HEAD was at the time of caching, to enable
/// cache invalidation.

/// Resolves a specified local git repo into a Target for analysis by Hipcheck
pub fn resolve_local_repo(
	phase: &SpinnerPhase,
	root: &Path,
	local_repo: LocalGitRepo,
) -> Result<Target> {
	let src = local_repo.path.clone();

	let specifier = src
		.to_str()
		.ok_or(hc_error!(
			"Path to local repo contained one or more invalid characters"
		))?
		.to_string();

	phase.update_status("copying");
	let local = clone_local_repo_to_cache(src.as_path(), root)?;
	// TODO - use git2 to set the local repo to the correct ref
	let _head = get_head_commit(&local).context("can't get head commit for local source")?;
	phase.update_status("trying to get remote");
	let remote = match try_resolve_remote_for_local(&local) {
		Ok(remote) => Some(remote),
		Err(err) => {
			log::debug!("failed to get remote [err='{}']", err);
			None
		}
	};

	Ok(Target {
		specifier,
		local: local_repo,
		remote,
		package: None,
	})
}

/// Creates a RemoteGitRepo struct from a given git URL by idenfitying if it is from a known host (currently only GitHub) or not
pub fn get_remote_repo_from_url(url: Url) -> Result<RemoteGitRepo> {
	match url.host() {
		Some(Host::Domain("github.com")) => {
			let (owner, repo) = get_github_owner_and_repo(&url)?;
			Ok(RemoteGitRepo {
				url,
				known_remote: Some(KnownRemote::GitHub { owner, repo }),
			})
		}
		Some(_) => Ok(RemoteGitRepo {
			url,
			known_remote: None,
		}),
		None => Err(hc_error!("Target repo URL is missing a host")),
	}
}

/// Resolves a remote git repo originally specified by its remote location into a Target for analysis by Hipcheck
pub fn resolve_remote_repo(
	phase: &SpinnerPhase,
	root: &Path,
	remote_repo: RemoteGitRepo,
	refspec: Option<String>,
) -> Result<Target> {
	// For remote repos originally specified by their URL, the specifier is just that URL
	let specifier = remote_repo.url.to_string();

	let path = match remote_repo.known_remote {
		Some(KnownRemote::GitHub {
			ref owner,
			ref repo,
		}) => pathbuf![root, "clones", "github", owner, repo],
		_ => {
			let clone_dir = build_unknown_remote_clone_dir(&remote_repo.url)
				.context("failed to prepare local clone directory")?;
			pathbuf![root, "clones", "unknown", &clone_dir]
		}
	};

	clone_or_update_remote(phase, &remote_repo.url, &path, refspec)?;

	let head = get_head_commit(&path)?;

	let local = LocalGitRepo {
		path,
		git_ref: head,
	};

	Ok(Target {
		specifier,
		local,
		remote: Some(remote_repo),
		package: None,
	})
}

/// Resolves a remote git repo derived from a source other than its remote location (e.g. a package or SPDX file) into a Target for analysis by Hipcheck
pub fn resolve_remote_package_repo(
	phase: &SpinnerPhase,
	root: &Path,
	remote_repo: RemoteGitRepo,
	specifier: String,
) -> Result<Target> {
	match remote_repo.known_remote {
		Some(KnownRemote::GitHub {
			ref owner,
			ref repo,
		}) => resolve_github_remote_repo(phase, root, remote_repo.clone(), owner, repo, specifier),
		_ => resolve_unknown_remote_repo(phase, root, remote_repo.clone(), specifier),
	}
}

fn resolve_github_remote_repo(
	phase: &SpinnerPhase,
	root: &Path,
	remote_repo: RemoteGitRepo,
	owner: &str,
	repo: &str,
	specifier: String,
) -> Result<Target> {
	let url = &remote_repo.url;

	let path = pathbuf![root, "clones", "github", owner, repo];

	clone_or_update_remote(phase, url, &path, None)?;

	let head = get_head_commit(&path)?;

	let local = LocalGitRepo {
		path,
		git_ref: head,
	};

	Ok(Target {
		specifier,
		local,
		remote: Some(remote_repo),
		package: None,
	})
}

fn resolve_unknown_remote_repo(
	phase: &SpinnerPhase,
	root: &Path,
	remote_repo: RemoteGitRepo,
	specifier: String,
) -> Result<Target> {
	let url = &remote_repo.url;

	let clone_dir =
		build_unknown_remote_clone_dir(url).context("failed to prepare local clone directory")?;
	let path = pathbuf![root, "clones", "unknown", &clone_dir];

	clone_or_update_remote(phase, url, &path, None)?;

	let head = get_head_commit(&path)?;

	let local = LocalGitRepo {
		path,
		git_ref: head,
	};

	Ok(Target {
		specifier,
		local,
		remote: Some(remote_repo),
		package: None,
	})
}

fn try_resolve_remote_for_local(local: &Path) -> Result<RemoteGitRepo> {
	let url = {
		let symbolic_ref = get_symbolic_ref(local)?;

		log::trace!("local source has symbolic ref [ref='{:?}']", symbolic_ref);

		if symbolic_ref.is_empty() {
			return Err(Error::msg("no symbolic ref found"));
		}

		let upstream = get_upstream_for_ref(local, &symbolic_ref)?;

		log::trace!("local source has upstream [upstream='{:?}']", upstream);

		if upstream.is_empty() {
			return Err(Error::msg("no upstream found"));
		}

		let remote = get_remote_from_upstream(&upstream)
			.ok_or_else(|| hc_error!("failed to get remote name from upstream '{}'", upstream))?;

		log::trace!("local source has remote [remote='{:?}']", remote);

		if remote.is_empty() {
			return Err(Error::msg("no remote found"));
		}

		let raw = get_url_for_remote(local, remote)?;

		log::trace!("local source remote has url [url='{}']", raw);

		if raw.is_empty() {
			return Err(Error::msg("no URL found for remote"));
		}

		Url::parse(&raw)?
	};

	let host = url
		.host_str()
		.ok_or_else(|| hc_error!("no host name in '{}'", url))?;

	match host {
		"github.com" => {
			let (owner, repo) = get_github_owner_and_repo(&url)?;
			Ok(RemoteGitRepo {
				url,
				known_remote: Some(KnownRemote::GitHub { owner, repo }),
			})
		}
		_ => Ok(RemoteGitRepo {
			url,
			known_remote: None,
		}),
	}
}

fn get_remote_from_upstream(upstream: &str) -> Option<&str> {
	upstream.split('/').next()
}

pub fn get_github_owner_and_repo(url: &Url) -> Result<(String, String)> {
	let mut segments = url
		.path_segments()
		.ok_or_else(|| Error::msg("GitHub URL missing path for owner and repository"))?;

	let owner = segments
		.next()
		.ok_or_else(|| Error::msg("GitHub URL missing owner"))?
		.to_owned();

	let repo = segments
		.next()
		.ok_or_else(|| Error::msg("GitHub URL missing repository"))?
		.trim_end_matches(".git")
		.to_owned();

	Ok((owner, repo))
}

#[allow(dead_code)]
fn get_github_owner_repo_and_pull_request(url: &Url) -> Result<(String, String, u64)> {
	let mut segments = url.path_segments().ok_or_else(|| {
		Error::msg("GitHub URL missing path for owner, repository, and pull request number")
	})?;

	let owner = segments
		.next()
		.ok_or_else(|| Error::msg("GitHub URL missing owner"))?
		.to_owned();

	let repo = segments
		.next()
		.ok_or_else(|| Error::msg("GitHub URL missing repository"))?
		.to_owned();

	let test_pull = segments.next();

	if test_pull == Some("pull") {
		let pull_request = segments
			.next()
			.ok_or_else(|| Error::msg("GitHub URL missing pull request number"))?
			.to_owned();
		let pull_request_number: u64 = pull_request.parse().unwrap();
		debug!("Pull request number: {}", pull_request_number);

		Ok((owner, repo, pull_request_number))
	} else {
		Err(Error::msg("GitHub URL not a pull request"))
	}
}

fn build_unknown_remote_clone_dir(url: &Url) -> Result<String> {
	let mut dir = String::new();

	// Add the host to the destination.
	// Unfortunately, due to borrowing issues, this is being recomputed here.
	let host = url
		.host_str()
		.ok_or_else(|| Error::msg("remote URL missing host"))?;
	dir.push_str(host);

	// Add each of the path segments.
	let segments = url
		.path_segments()
		.ok_or_else(|| Error::msg("remote URL missing path"))?;

	for segment in segments {
		dir.push_str("__");
		dir.push_str(segment);
	}

	Ok(dir)
}

fn clone_local_repo_to_cache(src: &Path, root: &Path) -> Result<PathBuf> {
	let src = src.canonicalize()?;
	let hc_data_root = pathbuf![root, "clones"];
	// If src dir is already in HC_CACHE/clones, leave it be. else clone from local fs
	if src.starts_with(&hc_data_root) {
		return Ok(src);
	}
	let dest = pathbuf![&hc_data_root, "local", src.file_name().unwrap()];
	if dest.exists() {
		std::fs::remove_dir_all(&dest)?;
	}
	let src_str = src
		.to_str()
		.ok_or_else(|| hc_error!("source isn't UTF-8 encoded '{}'", src.display()))?;
	let dest_str = dest
		.to_str()
		.ok_or_else(|| hc_error!("destination isn't UTF-8 encoded '{}'", dest.display()))?;
	let _output = GitCommand::new_repo(["clone", src_str, dest_str])?.output()?;
	Ok(dest)
}

pub fn clone_or_update_remote(
	phase: &SpinnerPhase,
	url: &Url,
	dest: &Path,
	refspec: Option<String>,
) -> Result<()> {
	if dest.exists() {
		phase.update_status("pulling");
		git::fetch(dest).context("failed to update remote repository")?;
	} else {
		phase.update_status("cloning");
		git::clone(url, dest).context("failed to clone remote repository")?;
	}
	git::checkout(dest, refspec)
}

fn get_symbolic_ref(dest: &Path) -> Result<String> {
	let output = GitCommand::for_repo(dest, ["symbolic-ref", "-q", "HEAD"])?
		.output()
		.context("Git failed to get symbolic ref for HEAD")?;

	Ok(output.trim().to_owned())
}

fn get_upstream_for_ref(dest: &Path, symbolic_ref: &str) -> Result<String> {
	let output = GitCommand::for_repo(
		dest,
		["for-each-ref", "--format=%(upstream:short)", symbolic_ref],
	)?
	.output()
	.context("Git failed to get name of upstream for HEAD")?;

	Ok(output.trim().to_owned())
}

fn get_url_for_remote(dest: &Path, remote: &str) -> Result<String> {
	let output = GitCommand::for_repo(dest, ["remote", "get-url", remote])?.output()?;

	Ok(output.trim().to_owned())
}

pub(crate) fn get_head_commit(path: &Path) -> Result<String> {
	let output = GitCommand::for_repo(path, ["rev-parse", "--short", "HEAD"])?.output()?;

	Ok(output.trim().to_owned())
}
