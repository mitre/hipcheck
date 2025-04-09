// SPDX-License-Identifier: Apache-2.0
use super::{
	cyclone_dx::{extract_cyclonedx_download_url, BomTarget},
	multi::resolve_package_lock_json,
	pm::{detect_and_extract, extract_repo_for_maven},
	spdx::extract_spdx_download_url,
};
use crate::{
	error::{Context, Result},
	hc_error,
	shell::spinner_phase::SpinnerPhase,
	source::{
		build_unknown_remote_clone_dir, clone_local_repo_to_cache, get_remote_repo_from_url, git,
		try_resolve_remote_for_local,
	},
	target::{multi::resolve_go_mod, types::*},
};
use git2::{AnnotatedCommit, Repository};
use pathbuf::pathbuf;
use regex::Regex;
use semver::Version;
use tokio_stream::StreamExt;
use url::Url;

use std::{
	fmt::Display,
	ops::Not,
	path::{Path, PathBuf},
	pin::Pin,
	sync::LazyLock,
};

// This module implements the behavior described in RFD 0005 for target
// resolution. The `TargetResolver` acts as a mutable superset of the fields of
// `Target`, the behavior of which is controlled by the `TargetResolverConfig`
// struct. Starting from the `seed`, the TargetResolver calls `resolve()`, which
// causes the seed to resolve to another `Option<T>` field in the TargetResolver
// struct definition. This field also implements `resolve()`, and in this way we
// can move towards an ultimate `LocalGitRepo` object while retaining knowledge
// of where we came from for use in things like deciding what default refspec to
// use (if any) and fuzzy version matching.

/// Control the behavior of a `TargetResolver` struct instance
#[derive(Debug, Clone)]
pub struct TargetResolverConfig {
	/// Object for updating the Hipcheck phase. If None, calls to
	/// `TargetResolver::update_status()` will be no-ops
	pub phase: Option<SpinnerPhase>,
	/// The root dir for the Hipcheck cache
	pub cache: PathBuf,
}

/// Contains the algorithm for progressively resolving a `TargetSeed` to a
/// `LocalGitRepo` in a context-aware fashion.
pub struct TargetResolver {
	// Leaving these top fields private allows us to prevent mutation in
	// `ResolveRepo` trait impls below
	config: TargetResolverConfig,
	seed: SingleTargetSeed,
	pub local: Option<LocalGitRepo>,
	pub remote: Option<RemoteGitRepo>,
	pub package: Option<Package>,
	pub maven: Option<MavenPackage>,
	pub sbom: Option<Sbom>,
}

impl TargetResolver {
	/// Replacement for `phase.update_status()` that allows us to not
	/// print anything if desired.
	pub fn update_status(&self, status: impl Display) {
		if let Some(phase) = &self.config.phase {
			phase.update_status(status);
		}
	}

	/// Accessor method to ensure immutability of `config` field
	pub fn get_config(&self) -> &TargetResolverConfig {
		&self.config
	}

	/// Accessor method to ensure immutability of `seed` field
	pub fn get_seed(&self) -> &SingleTargetSeed {
		&self.seed
	}

	/// Try to determine the correct refspec to check out, depending on the
	/// resolution history.
	pub fn get_checkout_target(&mut self, repo_path: &Path) -> Result<Option<String>> {
		let res = if let Some(refspec) = &self.seed.refspec {
			// if ref provided on CLI, use that
			Some(refspec.clone())
		} else if let Some(pkg) = &self.package {
			// Open the repo with git2.
			let repo: Repository = Repository::open(repo_path)?;

			let cmt = {
				// If the package was specified with a version, try fuzzy matching it with the repo tags
				if pkg.has_version() {
					// @Todo - add self.seed.ignore_version_errors, and if fuzzy match fails use "origin/HEAD"
					fuzzy_match_package_version(&repo, pkg)?
				}
				// No version was specified. Try to figure out the tag representing the latest version in the repo
				else if let Some(cmt) = {
					log::debug!("Package specified without version, trying to determine latest version tag in repo");
					try_find_commit_for_latest_version_tag(&repo)?
				} {
					cmt
				}
				// We've exhausted our heuristics, the user must provide a ref flag
				else {
					return Err(hc_error!("please provide --ref flag"));
				}
			};
			Some(format!("{}", cmt.id()))
		} else {
			use SingleTargetSeedKind::*;
			match &self.seed.kind {
				LocalRepo(_) => None,
				RemoteRepo(_) => Some("origin/HEAD".to_owned()),
				_ => {
					return Err(hc_error!("please provide --ref flag"));
				}
			}
		};
		Ok(res)
	}

	pub async fn resolve_map(input: (TargetResolverConfig, SingleTargetSeed)) -> Result<Target> {
		Self::resolve(input.0, input.1).await
	}

	/// Main function entrypoint for the resolution algorithm
	pub async fn resolve(config: TargetResolverConfig, seed: SingleTargetSeed) -> Result<Target> {
		#[cfg(feature = "print-timings")]
		let _0 = crate::benchmarking::print_scope_time!("resolve_target");

		let mut resolver = TargetResolver {
			config,
			seed: seed.clone(),
			local: None,
			remote: None,
			package: None,
			maven: None,
			sbom: None,
		};
		use SingleTargetSeedKind::*;
		// Resolution logic depends on seed
		let local = match seed.kind {
			Sbom(sbom) => {
				resolver.sbom = Some(sbom.clone());
				sbom.resolve(&mut resolver)
			}
			MavenPackage(maven) => {
				resolver.maven = Some(maven.clone());
				maven.resolve(&mut resolver)
			}
			Package(pkg) => {
				resolver.package = Some(pkg.clone());
				pkg.resolve(&mut resolver)
			}
			RemoteRepo(repo) => {
				resolver.remote = Some(repo.clone());
				repo.resolve(&mut resolver)
			}
			VcsUrl(vcs) => {
				resolver.remote = Some(vcs.remote.clone());
				vcs.remote.resolve(&mut resolver)
			}
			LocalRepo(local) => {
				resolver.local = Some(local.clone());
				local.resolve(&mut resolver)
			}
		}?;
		// Finally piece together the Target with the non-optional local repo
		Ok(Target {
			specifier: resolver.get_seed().specifier.clone(),
			local,
			remote: resolver.remote,
			package: resolver.package,
		})
	}
}

trait ResolveRepo {
	fn resolve(self, t: &mut TargetResolver) -> Result<LocalGitRepo>;
}

impl ResolveRepo for LocalGitRepo {
	fn resolve(mut self, t: &mut TargetResolver) -> Result<LocalGitRepo> {
		let cache_path = &t.get_config().cache;

		// If not already in cache, clone to cache
		if self.path.starts_with(cache_path).not() {
			t.update_status("copying repo to cache");
			log::debug!("Copying local repo to cache");
			self.path = clone_local_repo_to_cache(&self.path, &t.get_config().cache)?;
		} else {
			log::debug!("Local repo path already in cache");
		};

		// Ref we try to checkout is either from self or t
		let init_ref = if self.git_ref.is_empty().not() {
			log::debug!("Targeting existing `git_ref` field '{}'", &self.git_ref);
			Some(self.git_ref.clone())
		} else {
			let refspec = t.get_checkout_target(&self.path)?;
			log::debug!(
				"Existing `git_ref` field was empty, using git_ref '{:?}'",
				refspec
			);
			refspec
		};

		// Checkout specified ref
		self.git_ref = git::checkout(&self.path, init_ref)?;

		log::debug!("Resolved git ref was '{}'", &self.git_ref);

		// If not descendant of remote, try to resolve a remote
		if t.remote.is_none() {
			t.update_status("trying to get remote");
			t.remote = match try_resolve_remote_for_local(&self.path) {
				Ok(remote) => Some(remote),
				Err(err) => {
					log::debug!("failed to get remote [err='{}']", err);
					None
				}
			};
		}

		t.local = Some(self.clone());
		Ok(self)
	}
}

impl ResolveRepo for RemoteGitRepo {
	fn resolve(self, t: &mut TargetResolver) -> Result<LocalGitRepo> {
		let root = &t.get_config().cache;

		let path = match self.known_remote {
			Some(KnownRemote::GitHub {
				ref owner,
				ref repo,
			}) => pathbuf![root, "clones", "github", owner, repo],
			_ => {
				let clone_dir = build_unknown_remote_clone_dir(&self.url)
					.context("failed to prepare local clone directory")?;
				pathbuf![root, "clones", "unknown", &clone_dir]
			}
		};

		// Clone remote repo if not exists
		if path.exists().not() {
			t.update_status("cloning");
			git::clone(&self.url, &path)
				.map_err(|e| hc_error!("failed to clone remote repository {}", e))?;
		} else {
			t.update_status("pulling");
		}
		// Whether we cloned or not, we need to fetch so we get tags
		git::fetch(&path).context("failed to fetch updates from remote repository")?;

		let refspec = t.get_checkout_target(&path)?;
		let git_ref = git::checkout(&path, refspec)?;
		log::debug!("Resolved git ref was '{}'", &git_ref);

		let local = LocalGitRepo { path, git_ref };

		t.local = Some(local.clone());
		t.remote = Some(self);

		Ok(local)
	}
}

impl ResolveRepo for Package {
	fn resolve(self, t: &mut TargetResolver) -> Result<LocalGitRepo> {
		let url = detect_and_extract(&self).context("Could not get git repo URL for package")?;

		// Create Target for a remote git repo originating with a package
		let package_git_repo = get_remote_repo_from_url(url)?;

		package_git_repo.resolve(t)
	}
}

impl ResolveRepo for MavenPackage {
	fn resolve(self, t: &mut TargetResolver) -> Result<LocalGitRepo> {
		let url = extract_repo_for_maven(self.url.as_ref())
			.context("Could not get git repo URL for Maven package")?;

		// Create Target for a remote git repo originating with a Maven package
		let package_git_repo = get_remote_repo_from_url(url)?;

		package_git_repo.resolve(t)
	}
}

impl ResolveRepo for Sbom {
	fn resolve(self, t: &mut TargetResolver) -> Result<LocalGitRepo> {
		// Attempt to get the download location for the local SBOM package, using the function
		// appropriate to the SBOM standard.
		let source = self.path.to_str().ok_or(hc_error!(
			"SBOM path contained one or more invalid characters"
		))?;
		let bom_target = match self.standard {
			// converts the result from extract_spdx_download_url into the url variant of BomTarget
			// to make converting the bom target easier
			SbomStandard::Spdx => {
				let url = Url::parse(&extract_spdx_download_url(source)?)?;
				Ok(BomTarget::Url(url))
			}
			SbomStandard::CycloneDX => extract_cyclonedx_download_url(source),
		};
		match bom_target {
			Ok(BomTarget::Url(url)) => {
				let sbom_git_repo = get_remote_repo_from_url(url)?;
				t.remote = Some(sbom_git_repo.clone());
				sbom_git_repo.resolve(t)
			}
			Ok(BomTarget::Package(package)) => {
				t.package = Some(package.clone());
				package.resolve(t)
			}
			Err(error) => Err(hc_error!("Error, failed to parse package or url {}", error)),
		}
	}
}

fn fuzzy_match_package_version<'a>(
	repo: &'a Repository,
	package: &Package,
) -> Result<AnnotatedCommit<'a>> {
	let version = &package.version;
	let pkg_name = &package.name;

	log::debug!("Fuzzy matching package version '{version}'");

	let potential_tags = [
		version.clone(),
		format!("v{version}"),
		format!("{pkg_name}-{version}"),
		format!("{pkg_name}-v{version}"),
		format!("{pkg_name}_{version}"),
		format!("{pkg_name}_v{version}"),
		format!("{pkg_name}@{version}"), // NPM webpack-cli tags like this
		format!("{pkg_name}@v{version}"),
	];

	let mut opt_tgt_ref: Option<AnnotatedCommit> = None;
	for tag_str in potential_tags {
		if let Ok(obj) = repo.revparse_single(&tag_str) {
			log::debug!("revparse_single succeeded on '{}'", tag_str);
			opt_tgt_ref = Some(repo.find_annotated_commit(obj.peel_to_commit()?.id())?);
			break;
		} else {
			log::trace!("Tried and failed to find a tag '{tag_str}' in repo");
		}
	}

	let Some(tgt_ref) = opt_tgt_ref else {
		return Err(hc_error!(
			"Could not find in repo a refspec with any known combo of '{pkg_name}' and '{version}'"
		));
	};

	log::debug!("Resolved to commit: {}", tgt_ref.id());

	Ok(tgt_ref)
}

static SEMVER_REGEX: LazyLock<Regex> = LazyLock::new(|| {
	Regex::new(r"(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-((?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*)(?:\.(?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*))*))?(?:\+([0-9a-zA-Z-]+(?:\.[0-9a-zA-Z-]+)*))?").unwrap()
});

fn try_get_version_from_tag(opt_tag: Option<&str>) -> Option<(Version, String)> {
	if let Some(tag_str) = opt_tag {
		SEMVER_REGEX.captures(tag_str).and_then(|m| {
			Version::parse(m.get(0).unwrap().as_str())
				.ok()
				.map(|v| (v, tag_str.to_owned()))
		})
	} else {
		None
	}
}

// @SpeedUp - could reverse the `tag_names()` iterator and just find the first tag that matches the
// regex in `try_get_version_from_tag()`.
fn try_find_commit_for_latest_version_tag(
	repo: &Repository,
) -> Result<Option<AnnotatedCommit<'_>>> {
	// Iterate through the tags in the repo and filter for those that have a semver version embedded
	// in the name
	let mut tags: Vec<(Version, String)> = repo
		.tag_names(None)?
		.iter()
		.filter_map(try_get_version_from_tag)
		.collect();
	// Reverse-sort so "highest" version is first
	tags.sort_by(|a, b| b.0.cmp_precedence(&a.0));

	// Get the tag of the highest version and convert to an AnnotatedCommit
	if let Some((_, tag_str)) = tags.first() {
		log::debug!("Determined '{tag_str}' to be the tag for the newest version");
		if let Ok(obj) = repo.revparse_single(tag_str) {
			log::debug!("revparse_single succeeded on '{tag_str}'");
			Ok(Some(
				repo.find_annotated_commit(obj.peel_to_commit()?.id())?,
			))
		} else {
			let err_msg = format!("Failed to get commit for known tag '{}' in repo", tag_str);
			log::error!("{err_msg}");
			Err(hc_error!("{}", err_msg))
		}
	} else {
		log::debug!("No tags containing semver-compatible version numbers detected in repo");
		Ok(None)
	}
}

impl MultiTargetSeed {
	/// Parse and return all single target seeds from this multi target seed
	pub async fn get_target_seeds(&self) -> Result<Vec<SingleTargetSeed>> {
		match &self.kind {
			MultiTargetSeedKind::GoMod(path) => resolve_go_mod(path).await,
			MultiTargetSeedKind::PackageLockJson(path) => resolve_package_lock_json(path).await,
		}
	}
}

impl TargetSeed {
	/// Get all targets that are resolved from this seed
	pub async fn get_targets(
		&self,
		config: TargetResolverConfig,
	) -> Result<Pin<Box<impl StreamExt<Item = Result<Target>>>>> {
		let seed_vec = match self {
			TargetSeed::Single(single_target_seed) => vec![single_target_seed.clone()],
			TargetSeed::Multi(multi_target_seed) => multi_target_seed.get_target_seeds().await?,
		};
		let it = tokio_stream::iter(seed_vec)
			// Since `then()` can only take one arg, we combine the config and seed into a tuple
			.map(move |x| (config.clone(), x));
		Ok(Box::pin(it.then(TargetResolver::resolve_map)))
	}
}
