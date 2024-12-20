// SPDX-License-Identifier: Apache-2.0

use crate::{
	error::{Context, Result},
	hc_error,
	session::{
		cyclone_dx::extract_cyclonedx_download_url,
		pm::{detect_and_extract, extract_repo_for_maven},
		spdx::extract_spdx_download_url,
	},
	shell::spinner_phase::SpinnerPhase,
	source::{
		build_unknown_remote_clone_dir, clone_local_repo_to_cache, get_remote_repo_from_url, git,
		try_resolve_remote_for_local,
	},
	target::types::*,
};
use pathbuf::pathbuf;
use url::Url;

use std::{fmt::Display, ops::Not, path::PathBuf};

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
	seed: TargetSeed,
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
	pub fn get_seed(&self) -> &TargetSeed {
		&self.seed
	}

	/// Try to determine the correct refspec to check out, depending on the
	/// resolution history.
	pub fn get_checkout_target(&mut self) -> Result<Option<String>> {
		let res = if let Some(pkg) = &self.package {
			// @Todo - if version != "no version", try fuzzy match against repo, we know
			// we already have a local repo to use
			// if version fuzzy match fails, and self.seed.ignore_version_errors, use "origin/HEAD"
			Some(pkg.version.clone())
		} else if let Some(refspec) = &self.seed.refspec {
			// if ref provided on CLI, use that
			Some(refspec.clone())
		} else {
			use TargetSeedKind::*;
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

	/// Main function entrypoint for the resolution algorithm
	pub fn resolve(config: TargetResolverConfig, seed: TargetSeed) -> Result<Target> {
		let mut resolver = TargetResolver {
			config,
			seed: seed.clone(),
			local: None,
			remote: None,
			package: None,
			maven: None,
			sbom: None,
		};
		use TargetSeedKind::*;
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
			let refspec = t.get_checkout_target()?;
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

		// Clone or update remote repo
		if path.exists() {
			t.update_status("pulling");
			git::fetch(&path).context("failed to update remote repository")?;
		} else {
			t.update_status("cloning");
			git::clone(&self.url, &path).context("failed to clone remote repository")?;
		}

		let refspec = t.get_checkout_target()?;
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
		let source = self.path.to_str().ok_or(hc_error!(
			"SBOM path contained one or more invalid characters"
		))?;
		// Attempt to get the download location for the local SBOM package, using the function
		// appropriate to the SBOM standard
		let download_url = match self.standard {
			SbomStandard::Spdx => Url::parse(&extract_spdx_download_url(source)?)?,
			SbomStandard::CycloneDX => extract_cyclonedx_download_url(source)?,
		};

		// Create a Target for a remote git repo originating with an SBOM
		let sbom_git_repo = get_remote_repo_from_url(download_url)?;

		t.remote = Some(sbom_git_repo.clone());

		sbom_git_repo.resolve(t)
	}
}
