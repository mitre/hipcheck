// SPDX-License-Identifier: Apache-2.0

use crate::context::Context;
use crate::data::git_command::GitCommand;
use crate::error::Error;
use crate::error::Result;
use crate::hc_error;
use crate::shell::progress_phase::ProgressPhase;
use crate::shell::spinner_phase::SpinnerPhase;
use crate::shell::Shell;
pub use crate::source::query::*;
use crate::target::{KnownRemote, LocalGitRepo, RemoteGitRepo, Target};
use console::Term;
use git2::build::CheckoutBuilder;
use git2::build::RepoBuilder;
use git2::FetchOptions;
use git2::Progress;
use git2::RemoteCallbacks;
use log::debug;
use pathbuf::pathbuf;
use std::cell::OnceCell;
use std::fmt;
use std::fmt::Debug;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fs;
use std::io::Write;
use std::ops::Rem;
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

	let local = clone_local_repo_to_cache(src.as_path(), root)?;
	// TODO - use git2 to set the local repo to the correct ref
	let head = get_head_commit(&local).context("can't get head commit for local source")?;
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
) -> Result<Target> {
	// For remote repos originally specified by their URL, the specifier is just that URL
	let specifier = remote_repo.url.to_string();

	match remote_repo.known_remote {
		Some(KnownRemote::GitHub {
			ref owner,
			ref repo,
		}) => resolve_github_remote_repo(phase, root, remote_repo.clone(), owner, repo, specifier),
		_ => resolve_unknown_remote_repo(phase, root, remote_repo.clone(), specifier),
	}
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

	clone_or_update_remote(phase, url, &path)?;

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

	clone_or_update_remote(phase, url, &path)?;

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

// TODO: Remove reliance on SourceRepo in Hipcheck so we can delete this struct in favor of exclusively useing Target and related types

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
}

impl Display for SourceRepo {
	fn fmt(&self, f: &mut Formatter) -> fmt::Result {
		write!(f, "{}", self.raw)
	}
}

// TODO: Delete unused code related to analyzing targets other than repos
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
		phase: &SpinnerPhase,
		root: &Path,
		raw: &str,
	) -> Result<SourceChangeRequest> {
		let local = PathBuf::from(raw);

		let source = match (local.exists(), local.is_dir()) {
			// It's a local file, not a dir.
			(true, false) => Err(hc_error!(
				"source path is not a directory '{}'",
				local.display()
			)),
			// It's a local dir.
			(true, true) => SourceChangeRequest::resolve_local_change_request(raw, local),
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
	fn resolve_local_change_request(raw: &str, local: PathBuf) -> Result<SourceChangeRequest> {
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
		phase: &SpinnerPhase,
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
		phase: &SpinnerPhase,
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

pub fn clone_or_update_remote(phase: &SpinnerPhase, url: &Url, dest: &Path) -> Result<()> {
	if dest.exists() {
		phase.update_status("pulling");
		update_remote(dest).context("failed to update remote repository")
	} else {
		phase.update_status("cloning");
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

	// Create progress phases for recieving the objects and and resolving deltas.
	let transfer_phase: OnceCell<ProgressPhase> = OnceCell::new();
	let resolution_phase: OnceCell<ProgressPhase> = OnceCell::new();
	let checkout_phase: OnceCell<ProgressPhase> = OnceCell::new();

	// Create a struct to hold the callbacks for cloning the repo.
	let mut callbacks = RemoteCallbacks::new();

	// Messages from the remote ("Counting objects" etc) are sent over the sideband.
	// This involves clearing and replacing the line in many cases -- use console to do this effectively.
	callbacks.sideband_progress(|msg: &[u8]| {
		Shell::in_suspend(|| {
			// use the standard output.
			let mut term = Term::stdout();

			// Crash on errors here, since they should be relatively uncommon.
			term.clear_line().expect("clear line on standard output");

			write!(&mut term, "remote: {}", String::from_utf8_lossy(msg))
				.expect("wrote to standard output");

			term.flush().expect("flushed standard output");
		});

		true
	});

	callbacks.transfer_progress(|prog: Progress| {
		if prog.received_objects() > 0 {
			let phase = transfer_phase.get_or_init(|| {
				ProgressPhase::start(prog.total_objects() as u64, "(git) recieving objects")
			});

			phase.set_position(prog.received_objects() as u64);

			if prog.received_objects() == prog.total_objects() && !phase.is_finished() {
				phase.finish_successful(false);
			}
		}

		if prog.indexed_deltas() > 0 {
			let phase = resolution_phase.get_or_init(|| {
				ProgressPhase::start(prog.total_deltas() as u64, "(git) resolving deltas")
			});

			phase.set_position(prog.indexed_deltas() as u64);

			if prog.indexed_deltas() == prog.total_deltas() && !phase.is_finished() {
				phase.finish_successful(false);
			}
		}

		true
	});

	// Wrap the callbacks into the fetch options we pass to the repo builder.
	let mut fetch_opts = FetchOptions::new();

	fetch_opts
		// Use the remote callbacks for transfer.
		.remote_callbacks(callbacks)
		// Don't download any tags.
		.download_tags(git2::AutotagOption::None);

	// Create a struct to hold callbacks while checking out the cloned repo.
	let mut checkout_opts = CheckoutBuilder::new();

	// We don't care about the path being resolved, only the total and current numbers.
	checkout_opts.progress(|path, current, total| {
		// Initialize the phase if we haven't already.
		let phase =
			checkout_phase.get_or_init(|| ProgressPhase::start(total as u64, "(git) checkout"));

		// Set the bar to have the amount of progress in resolving.
		phase.set_position(current as u64);
		// Set the progress bar's status to the path being resolved.
		phase.update_status(
			path.map(Path::to_string_lossy)
				.unwrap_or("resolving...".into()),
		);

		// If we have resolved everything, finish the phase.
		if current == total {
			phase.finish_successful(false);
		}
	});

	RepoBuilder::new()
		.with_checkout(checkout_opts)
		.fetch_options(fetch_opts)
		.clone(url.as_str(), dest)?;

	Ok(())
}

pub(crate) fn get_head_commit(path: &Path) -> Result<String> {
	let output = GitCommand::for_repo(path, ["rev-parse", "--short", "HEAD"])?.output()?;

	Ok(output.trim().to_owned())
}

impl From<RemoteGitRepo> for Remote {
	fn from(value: RemoteGitRepo) -> Self {
		let mut o = "".to_owned();
		let mut r = "".to_owned();
		if let Some(KnownRemote::GitHub { owner, repo }) = value.known_remote {
			o = owner;
			r = repo;
		};
		Remote::GitHub {
			owner: o,
			repo: r,
			url: value.url,
		}
	}
}

impl TryFrom<Target> for Source {
	type Error = crate::error::Error;
	fn try_from(value: Target) -> Result<Self> {
		let raw = value.specifier;
		let local = value.local.path;
		let remote = value.remote.map(Remote::from);
		let head = get_head_commit(&local).context("can't get head commit for local source")?;
		Ok(Source {
			kind: SourceKind::Repo(SourceRepo {
				raw,
				local,
				remote,
				head,
			}),
		})
	}
}
