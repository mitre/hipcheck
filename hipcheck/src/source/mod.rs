// SPDX-License-Identifier: Apache-2.0

pub mod git;

use crate::{
	error::{Context, Error, Result},
	hc_error,
	target::{KnownRemote, RemoteGitRepo},
	util::git::GitCommand,
};
use pathbuf::pathbuf;
use std::path::{Path, PathBuf};
use url::{Host, Url};

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

pub fn try_resolve_remote_for_local(local: &Path) -> Result<RemoteGitRepo> {
	let url = {
		let symbolic_ref = get_symbolic_ref(local)?;

		tracing::trace!("local source has symbolic ref [ref='{:?}']", symbolic_ref);

		if symbolic_ref.is_empty() {
			return Err(Error::msg("no symbolic ref found"));
		}

		let upstream = get_upstream_for_ref(local, &symbolic_ref)?;

		tracing::trace!("local source has upstream [upstream='{:?}']", upstream);

		if upstream.is_empty() {
			return Err(Error::msg("no upstream found"));
		}

		let remote = get_remote_from_upstream(&upstream)
			.ok_or_else(|| hc_error!("failed to get remote name from upstream '{}'", upstream))?;

		tracing::trace!("local source has remote [remote='{:?}']", remote);

		if remote.is_empty() {
			return Err(Error::msg("no remote found"));
		}

		let raw = get_url_for_remote(local, remote)?;

		tracing::trace!("local source remote has url [url='{}']", raw);

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

pub fn build_unknown_remote_clone_dir(url: &Url) -> Result<String> {
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

pub fn clone_local_repo_to_cache(src: &Path, root: &Path) -> Result<PathBuf> {
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
