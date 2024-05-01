// SPDX-License-Identifier: Apache-2.0

mod query;

pub use query::*;

use hc_common::log::{self, debug};
use hc_common::{
	context::Context,
	error::{Error, Result},
	hc_error, pathbuf,
	url::Url,
};
use hc_git_command::GitCommand;
use hc_shell::Phase;
use std::ffi::OsStr;
use std::fmt::{self, Debug, Display, Formatter};
use std::fs;
use std::path::{Path, PathBuf};

// Right now our only subject of analysis is a repository, represented by the
// `Source` type. It may optionally include a `Remote` component, in which
// case analyses which require that remote may work! Now, we're faced with the
// question of how to handle supporting more subjects of analysis, namely
// pull requests, but also possibly to include individual commits or patches.

/// Represents the Git repository to be analyzed.
#[derive(Debug, PartialEq, Eq)]
pub struct SourceRepo {
	/// For printing the source
	raw: String,
	/// The local directory source
	local: PathBuf,
	/// Possibly a remote source as well (used for some analyses)
	remote: Option<Remote>,
	/// The head commit (for caching / cache invalidation)
	head: String,
}

impl SourceRepo {
	/// Resolving is how we ensure we have a valid, ready-to-go source of Git data
	/// for the rest of Hipcheck's analysis.
	///
	/// This function works by identifying if we have a local or remote source. If it's
	/// local, it'll just work with the local repository without cloning (all operations
	/// are write-only, so this won't harm the repo at all). If it's a remote source,
	/// Hipcheck will clone the source so it can work with a local copy, putting the
	/// clone in '<root>/clones'. It also notes whether a remote repo is from
	/// a known or unknown host, because some forms of analysis rely on accessing the
	/// API's of certain known hosts (currently just GitHub).
	///
	/// In either case, it also gets the commit head of the HEAD commit, so we can
	/// make sure future operations are all done relative to the HEAD, and that any
	/// cached data records what the HEAD was at the time of caching, to enable
	/// cache invalidation.
	pub fn resolve_repo(phase: &mut Phase, root: &Path, raw: &OsStr) -> Result<SourceRepo> {
		let local = PathBuf::from(raw);
		let raw = raw
			.to_str()
			.ok_or_else(|| Error::msg("source isn't UTF-8 encoded"))?;

		let source = match (local.exists(), local.is_dir()) {
			// It's a local file, not a dir.
			(true, false) => Err(hc_error!(
				"source path is not a directory '{}'",
				local.display()
			)),
			// It's a local dir.
			(true, true) => SourceRepo::resolve_local_repo(phase, raw, local),
			// It's possibly a remote URL.
			(false, _) => SourceRepo::resolve_remote_repo(phase, root, raw),
		};

		log::debug!("resolved source [source={:?}]", source);

		source
	}

	/// Get the path to the location of the repository on the local disk.
	pub fn local(&self) -> &Path {
		&self.local
	}

	/// If the source was remote, get the information on where it's from.
	pub fn remote(&self) -> Option<&Remote> {
		self.remote.as_ref()
	}

	/// Get the HEAD commit of the repository.
	pub fn head(&self) -> &str {
		&self.head
	}

	/// Get the name of the repository for the purpose of reporting to the user.
	pub fn name(&self) -> &str {
		// The field is called 'raw' because it's just whatever was passed in initially,
		// but we call it 'name' here because it's nicer.
		&self.raw
	}

	fn resolve_local_repo(_phase: &mut Phase, raw: &str, local: PathBuf) -> Result<SourceRepo> {
		let head = get_head_commit(&local).context("can't get head commit for local source")?;
		let remote = match SourceRepo::try_resolve_remote_for_local(&local) {
			Ok(remote) => Some(remote),
			Err(err) => {
				log::debug!("failed to get remote [err='{}']", err);
				None
			}
		};

		Ok(SourceRepo {
			raw: raw.to_string(),
			local,
			remote,
			head,
		})
	}

	fn resolve_remote_repo(phase: &mut Phase, root: &Path, raw: &str) -> Result<SourceRepo> {
		let url = Url::parse(raw)?;
		let host = url
			.host_str()
			.ok_or_else(|| hc_error!("source URL is missing a host '{}'", raw))?;

		match host {
			// It's a GitHub URL.
			"github.com" => SourceRepo::resolve_github_remote_repo(phase, root, raw, url),
			// It's another host.
			_ => SourceRepo::resolve_unknown_remote_repo(phase, root, raw, url),
		}
	}

	fn resolve_github_remote_repo(
		phase: &mut Phase,
		root: &Path,
		raw: &str,
		url: Url,
	) -> Result<SourceRepo> {
		let (owner, repo) = get_github_owner_and_repo(&url)
			.context("can't identify GitHub repository and owner")?;
		let dest = pathbuf![root, "clones", "github", &owner, &repo];

		clone_or_update_remote(phase, &url, &dest)?;

		let head = get_head_commit(&dest)?;

		Ok(SourceRepo {
			raw: raw.to_string(),
			local: dest,
			remote: Some(Remote::GitHub { owner, repo, url }),
			head,
		})
	}

	fn resolve_unknown_remote_repo(
		phase: &mut Phase,
		root: &Path,
		raw: &str,
		url: Url,
	) -> Result<SourceRepo> {
		let clone_dir = build_unknown_remote_clone_dir(&url)
			.context("failed to prepare local clone directory")?;
		let dest = pathbuf![root, "clones", "unknown", &clone_dir];

		clone_or_update_remote(phase, &url, &dest)?;

		let head = get_head_commit(&dest)?;

		Ok(SourceRepo {
			raw: raw.to_string(),
			local: dest,
			remote: Some(Remote::Unknown(url)),
			head,
		})
	}

	fn try_resolve_remote_for_local(local: &Path) -> Result<Remote> {
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

			let remote = get_remote_from_upstream(&upstream).ok_or_else(|| {
				hc_error!("failed to get remote name from upstream '{}'", upstream)
			})?;

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
				Ok(Remote::GitHub { owner, repo, url })
			}
			_ => Ok(Remote::Unknown(url)),
		}
	}
}

impl Display for SourceRepo {
	fn fmt(&self, f: &mut Formatter) -> fmt::Result {
		write!(f, "{}", self.raw)
	}
}

#[allow(dead_code)]
#[derive(Debug, PartialEq, Eq)]
pub struct SourceChangeRequest {
	id: u64,
	// The repository associated with the change request, guaranteed to
	// have a GitHub component.
	source_repo: SourceRepo,
}

impl SourceChangeRequest {
	/// As above, but for a single pull request
	#[allow(dead_code)]
	pub fn resolve_change_request(
		phase: &mut Phase,
		root: &Path,
		raw: &OsStr,
	) -> Result<SourceChangeRequest> {
		let local = PathBuf::from(raw);
		let raw = raw
			.to_str()
			.ok_or_else(|| Error::msg("source isn't UTF-8 encoded"))?;

		let source = match (local.exists(), local.is_dir()) {
			// It's a local file, not a dir.
			(true, false) => Err(hc_error!(
				"source path is not a directory '{}'",
				local.display()
			)),
			// It's a local dir.
			(true, true) => SourceChangeRequest::resolve_local_change_request(phase, raw, local),
			// It's possibly a remote URL.
			(false, _) => SourceChangeRequest::resolve_remote_change_request(phase, root, raw),
		};

		log::debug!("resolved change request source [source='{:?}']", source);

		source
	}

	/// Get the path to the location of the repository on the local disk.
	#[allow(dead_code)]
	pub fn local(&self) -> &Path {
		&self.source_repo.local
	}

	/// If the source was remote, get the information on where it's from.
	#[allow(dead_code)]
	pub fn remote(&self) -> Option<&Remote> {
		self.source_repo.remote.as_ref()
	}

	/// Get the HEAD commit of the repository.
	#[allow(dead_code)]
	pub fn head(&self) -> &str {
		&self.source_repo.head
	}

	// /// Get the name of the repository for the purpose of reporting to the user.
	#[allow(dead_code)]
	pub fn name(&self) -> &str {
		// The field is called 'raw' because it's just whatever was passed in initially,
		// but we call it 'name' here because it's nicer.
		&self.source_repo.raw
	}

	#[allow(dead_code)]
	fn resolve_local_change_request(
		_phase: &mut Phase,
		raw: &str,
		local: PathBuf,
	) -> Result<SourceChangeRequest> {
		let head = get_head_commit(&local).context("can't get head commit for local source")?;
		let (remote, pull_request_id) = SourceChangeRequest::try_resolve_remote_for_local(&local)?;

		Ok(SourceChangeRequest {
			id: pull_request_id,
			source_repo: SourceRepo {
				raw: raw.to_string(),
				local,
				remote: Some(remote),
				head,
			},
		})
	}

	fn resolve_remote_change_request(
		phase: &mut Phase,
		root: &Path,
		raw: &str,
	) -> Result<SourceChangeRequest> {
		let url = Url::parse(raw)?;
		let host = url
			.host_str()
			.ok_or_else(|| hc_error!("source URL is missing a host '{}'", raw))?;

		match host {
			// It's a GitHub URL.
			"github.com" => {
				SourceChangeRequest::resolve_github_remote_change_request(phase, root, raw, url)
			}
			// It's another host.
			_ => Err(Error::msg("host is not GitHub")),
		}
	}

	#[allow(dead_code)]
	fn resolve_github_remote_change_request(
		phase: &mut Phase,
		root: &Path,
		raw: &str,
		url: Url,
	) -> Result<SourceChangeRequest> {
		let (owner, repo, pull_request_id) = get_github_owner_repo_and_pull_request(&url)
			.context("can't identify GitHub repository, owner, and pull_request_number")?;

		let dest = pathbuf![root, "clones", "github", &owner, &repo];

		let repo_url = Url::parse(&format!("https://github.com/{}/{}", owner, repo))?;

		clone_or_update_remote(phase, &repo_url, &dest)?;

		let head = get_head_commit(&dest)?;

		Ok(SourceChangeRequest {
			id: pull_request_id,
			source_repo: SourceRepo {
				raw: raw.to_string(),
				local: dest,
				remote: Some(Remote::GitHub { owner, repo, url }),
				head,
			},
		})
	}

	#[allow(dead_code)]
	fn try_resolve_remote_for_local(local: &Path) -> Result<(Remote, u64)> {
		let url = {
			let symbolic_ref = get_symbolic_ref(local)?;

			log::debug!("request source has symbolic ref [ref='{:?}']", symbolic_ref);

			if symbolic_ref.is_empty() {
				return Err(Error::msg("no symbolic ref found"));
			}

			let upstream = get_upstream_for_ref(local, &symbolic_ref)?;

			log::debug!("request source has upstream [upstream='{:?}']", upstream);

			if upstream.is_empty() {
				return Err(Error::msg("no upstream found"));
			}

			let remote = get_remote_from_upstream(&upstream).ok_or_else(|| {
				hc_error!("failed to get remote name from upstream '{}'", upstream)
			})?;

			log::debug!("request source has remote [remote='{:?}']", remote);

			if remote.is_empty() {
				return Err(Error::msg("no remote found"));
			}

			let raw = get_url_for_remote(local, remote)?;

			log::debug!("request source has url [url='{}']", raw);

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
				let (owner, repo, pull_request_number) =
					get_github_owner_repo_and_pull_request(&url)?;
				Ok((Remote::GitHub { owner, repo, url }, pull_request_number))
			}
			_ => Ok((Remote::Unknown(url), 0)),
		}
	}
}
type CommitHash = String;

#[allow(dead_code)]
struct SourceCommit {
	hash: CommitHash,
	source_repo: SourceRepo,
}

trait AsSourceRepo {
	fn as_source_repo(&self) -> &SourceRepo;
}

impl AsSourceRepo for SourceRepo {
	fn as_source_repo(&self) -> &SourceRepo {
		self
	}
}

impl AsSourceRepo for SourceChangeRequest {
	fn as_source_repo(&self) -> &SourceRepo {
		&self.source_repo
	}
}

impl AsSourceRepo for SourceCommit {
	fn as_source_repo(&self) -> &SourceRepo {
		&self.source_repo
	}
}

#[derive(Debug, PartialEq, Eq)]
pub struct Source {
	pub kind: SourceKind,
}

#[allow(dead_code)]
#[derive(Debug, PartialEq, Eq)]
pub enum SourceKind {
	Repo(SourceRepo),
	ChangeRequest(SourceChangeRequest),
}

impl Source {
	pub fn get_repo(&self) -> &SourceRepo {
		use SourceKind::*;

		match &self.kind {
			Repo(r) => r.as_source_repo(),
			ChangeRequest(cr) => cr.as_source_repo(),
		}
	}

	/// Makes sure that when we get a Remote, we get the correct kind.
	// If the Source is something other than a repo, the Remote is created from its constituent elements
	pub fn get_remote(&self) -> Option<Remote> {
		use SourceKind::*;

		match &self.kind {
			Repo(r) => r.remote().cloned(),
			ChangeRequest(cr) => Some(Remote::GitHubPr {
				owner: cr
					.source_repo
					.remote()?
					.to_owned()
					.owner()
					.ok()?
					.to_string(),
				repo: cr.source_repo.remote()?.to_owned().repo().ok()?.to_string(),
				pull_request: cr.id,
				url: cr.source_repo.remote()?.to_owned().url().to_owned(),
			}),
		}
	}
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Remote {
	GitHub {
		owner: String,
		repo: String,
		url: Url,
	},
	GitHubPr {
		owner: String,
		repo: String,
		pull_request: u64,
		url: Url,
	},
	Unknown(Url),
}

impl Remote {
	fn owner(&self) -> Result<&String> {
		match self {
			Remote::GitHub { owner, .. } => Ok(owner),
			Remote::GitHubPr { owner, .. } => Ok(owner),
			Remote::Unknown(..) => Err(Error::msg("Not a github.com remote source")),
		}
	}
	fn repo(&self) -> Result<&String> {
		match self {
			Remote::GitHub { repo, .. } => Ok(repo),
			Remote::GitHubPr { repo, .. } => Ok(repo),
			Remote::Unknown(..) => Err(Error::msg("Not a github.com remote source")),
		}
	}
	pub fn url(&self) -> &Url {
		match self {
			Remote::GitHub { url, .. } => url,
			Remote::GitHubPr { url, .. } => url,
			Remote::Unknown(url) => url,
		}
	}
}

impl Display for Remote {
	fn fmt(&self, f: &mut Formatter) -> fmt::Result {
		write!(f, "{}", self.url())
	}
}

fn get_remote_from_upstream(upstream: &str) -> Option<&str> {
	upstream.split('/').next()
}

fn get_github_owner_and_repo(url: &Url) -> Result<(String, String)> {
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

fn clone_or_update_remote(phase: &mut Phase, url: &Url, dest: &Path) -> Result<()> {
	if dest.exists() {
		phase.update("pulling")?;
		update_remote(dest).context("failed to update remote repository")
	} else {
		phase.update("cloning")?;
		clone_remote(url, dest).context("failed to clone remote repository")
	}
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

fn update_remote(dest: &Path) -> Result<()> {
	let _output = GitCommand::for_repo(dest, ["pull"])?.output()?;

	Ok(())
}

fn clone_remote(url: &Url, dest: &Path) -> Result<()> {
	log::debug!("remote repository cloning url is {}", url);
	fs::create_dir_all(dest).context(format!(
		"can't make local clone directory '{}'",
		dest.display()
	))?;

	let dest = dest
		.to_str()
		.ok_or_else(|| hc_error!("destination isn't UTF-8 encoded '{}'", dest.display()))?;

	let _output = GitCommand::new_repo([
		"clone",
		"-q",
		"--single-branch",
		"--no-tags",
		url.as_str(),
		dest,
	])?
	.output()?;

	Ok(())
}

pub(crate) fn get_head_commit(path: &Path) -> Result<String> {
	let output = GitCommand::for_repo(path, ["rev-parse", "--short", "HEAD"])?.output()?;

	Ok(output.trim().to_owned())
}
