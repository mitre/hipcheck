// SPDX-License-Identifier: Apache-2.0

use crate::{
	cli::format_npm_url,
	error::{Error, Result},
	hc_error, source,
	target::{Package, PackageHost, SingleTargetSeed, SingleTargetSeedKind},
	util::http::agent,
};

use gomod_rs::{Context as GmContext, Directive, Identifier};
use regex::Regex;
use serde::Deserialize;
use serde_json::from_slice;
use url::Url;

use std::{collections::BTreeMap, ops::Deref, path::Path, sync::LazyLock};

/// A struct with owned data to simplify TryFrom conversion impl
/// vs. gomod_rs::Context which is full of lifetimes
struct GoModRequire {
	repo: String,
	version: String,
}

#[cfg(test)]
impl GoModRequire {
	pub fn new(repo: &str, version: &str) -> Self {
		GoModRequire {
			repo: repo.to_owned(),
			version: version.to_owned(),
		}
	}
}

impl<'a> From<&GmContext<'a, (&'a str, Identifier<'a>)>> for GoModRequire {
	fn from(value: &GmContext<'a, (&'a str, Identifier<'a>)>) -> GoModRequire {
		GoModRequire {
			repo: value.value.0.to_owned(),
			version: value.value.1.deref().to_owned(),
		}
	}
}

/// For a given module path spec, search for any path components that end in `.`
/// followed by one of the Go-supported VCS qualifiers. If found, return everything
/// in the string up to the `.` preceding the VCS qualifier as the repo URL.
fn try_get_vcs_qual_base(m: &str) -> Option<&str> {
	const GO_VSC_QUALS: [&str; 5] = ["git", "bzr", "fossil", "hg", "svn"];

	let opt_match = m
		.match_indices(".")
		.filter_map(|(i, _)| {
			// The index after the dot
			let j = i + 1;
			// Once we have a '.', see if the remaining string starts with any of GO_VCS_QUALS
			let opt_var = GO_VSC_QUALS
				.iter()
				// find a vcs qualifier such that m[j..] starts with it, and the next char after
				// is either None or a '/' (aka it either ends a path component or ends the entire
				// string)
				.find(|x| {
					m[j..].starts_with(*x)
						&& m.get(j + x.len()..).is_none_or(|y| y.starts_with("/"))
				});
			// Return j + the len of the qual to get the char right after the qual
			opt_var.map(|x| j + x.len())
		})
		.next();
	// If we found a match, return everything up to the match
	opt_match.map(|i| &m[0..i])
}

/// In a string containing raw HTML, find a `<meta>` tag with Go import information.
fn try_get_repo_url_from_go_get(page_str: &str) -> Option<&str> {
	static GO_META_REGEX: LazyLock<Regex> = LazyLock::new(|| {
		Regex::new("<meta name=\"go-import\" content=\"(\\S+) (\\S+) (\\S+)\">").unwrap()
	});
	// Find first occurence of regex and pull out repo url as str
	GO_META_REGEX
		.captures(page_str)
		.and_then(|m| m.get(3).map(|s| s.as_str()))
}

impl TryFrom<GoModRequire> for SingleTargetSeed {
	type Error = Error;

	fn try_from(value: GoModRequire) -> Result<SingleTargetSeed> {
		// Based on go module resolution described here: https://go.dev/ref/mod#vcs-find

		let mut repo = value.repo;
		let mut version = value.version;

		let specifier = format!("{repo} {version}");

		// Check if version string is really a pseudo-version, if so, pull out ref hash
		// Reference: https://go.dev/ref/mod#pseudo-versions
		if let Some((_, hash)) = version.rsplit_once("-") {
			if hash.len() == 12 {
				version = hash.to_owned();
			}
		}

		let is_github = repo.contains("github.com");

		// go dependency URLs tend not to have an https:// marking
		// Doing this now may not be correct in the long run
		if !repo.starts_with("http") {
			repo = format!("https://{repo}");
		}

		let remote_repo_url: String = if is_github {
			// If repo is in the github domain, we just use the passed URL
			repo
		} else {
			// Otherwise check for any vcs qualifiers ending path components (e.g. foo/file.git/bar)
			match try_get_vcs_qual_base(&repo) {
				Some(vcs_base) => {
					if !vcs_base.ends_with(".git") {
						return Err(hc_error!(
						"Only git repos are supported currently, got Go dependency path {vcs_base}"
					));
					}
					vcs_base.to_owned()
				}
				// No vcs quals, have to query URL with ?go-get=1 to receive the go-import tag with
				// repo url info
				None => {
					let get_url = format!("{repo}?go-get=1");
					let html_str = agent::agent()
						.get(&get_url)
						.call()
						.map_err(|e| hc_error!("go dependency GET request failed: {}", e))?
						.into_string()
						.map_err(|e| {
							hc_error!(
								"failed to read body of response to go dependency request: {}",
								e
							)
						})?;
					// Find first occurence of go-import meta tag and pull out repo url as str
					let url_str: &str = try_get_repo_url_from_go_get(&html_str)
						.ok_or(hc_error!("Failed to find repo url in go-get HTML page"))?;
					url_str.to_owned()
				}
			}
		};

		let url = Url::parse(&remote_repo_url).map_err(|e| hc_error!("URL parse failed: {}", e))?;
		let remote_repo = source::get_remote_repo_from_url(url)?;

		Ok(SingleTargetSeed {
			kind: SingleTargetSeedKind::RemoteRepo(remote_repo),
			refspec: Some(version),
			specifier,
		})
	}
}

impl<'a> TryFrom<&GmContext<'a, (&'a str, Identifier<'a>)>> for SingleTargetSeed {
	type Error = Error;

	fn try_from(value: &GmContext<'a, (&'a str, Identifier<'a>)>) -> Result<SingleTargetSeed> {
		GoModRequire::from(value).try_into()
	}
}

/// Read from a `go.mod` file at `path`, parse dependencies from `requires` sections
/// and parse each into a `SingleTargetSeed` appropriate for further target resolution.
pub(crate) async fn resolve_go_mod(path: &Path) -> Result<Vec<SingleTargetSeed>> {
	let raw_content = tokio::fs::read_to_string(path)
		.await
		.map_err(|e| hc_error!("Failed to load go.mod target seed: {}", e))?;
	let gomod = gomod_rs::parse_gomod(&raw_content)
		.map_err(|e| hc_error!("go.mod parsing failed: {}", e))?;
	gomod
		.iter()
		// Extract the dependencies list from the file
		.filter_map(|x| match x {
			GmContext {
				value: Directive::Require { specs },
				..
			} => Some(specs),
			_ => None,
		})
		// Impl allows for multiple `require` blocks so need to flatten their contents
		.flatten()
		// Map single dependency spec to a TargetSeed
		.map(TryInto::<SingleTargetSeed>::try_into)
		.collect::<Result<Vec<SingleTargetSeed>>>()
}

#[derive(Deserialize)]
pub struct PackageLockJson {
	pub dependencies: Option<BTreeMap<String, PackageDependency>>,
	// ignore packages
}

#[derive(Deserialize)]
pub struct PackageDependency {
	pub version: String,
}

pub(crate) async fn resolve_package_lock_json(path: &Path) -> Result<Vec<SingleTargetSeed>> {
	// Parse package-lock.json
	let contents = tokio::fs::read(path)
		.await
		.map_err(|e| hc_error!("Failed to read package-lock.json: {}", e))?;
	let package_lock: PackageLockJson =
		from_slice(&contents).map_err(|e| hc_error!("Failed to parse package-lock.json: {}", e))?;

	// Extract dependencies from file
	let dependencies: BTreeMap<String, SingleTargetSeed> = package_lock
		.dependencies
		.iter()
		.flat_map(|dependencies| {
			dependencies.iter().map(|(name, dependency)| {
				// Map dependency to SingleTargetSeedKind::Package
				let name = name.to_string();
				let version = dependency.version.clone();
				let specifier = format!("{name} {version}");

				let purl = Url::parse(&format_npm_url(&name, &version))
					.map_err(|e| hc_error!("Failed to parse dependency url: {}", e))?;

				Ok((
					name.clone(),
					SingleTargetSeed {
						kind: SingleTargetSeedKind::Package(Package {
							purl,
							name,
							version: version.clone(),
							host: PackageHost::Npm,
						}),
						refspec: None,
						specifier,
					},
				))
			})
		})
		.collect::<Result<BTreeMap<String, SingleTargetSeed>>>()?;

	// collect vals from BTreeMap to Vec
	let dependencies_vec: Vec<SingleTargetSeed> = dependencies.into_values().collect();

	Ok(dependencies_vec)
}

#[cfg(test)]
mod tests {
	use std::path::PathBuf;

	use url::Url;

	use crate::target::{
		multi::resolve_package_lock_json, Package, PackageHost, SingleTargetSeed,
		SingleTargetSeedKind,
	};

	use super::*;
	use crate::target::KnownRemote;

	#[test]
	fn test_vcs_detection() {
		let i = "golang.org/x.fossil/mod";
		assert_eq!(try_get_vcs_qual_base(i), Some("golang.org/x.fossil"));
		let i = "example.com/foo.git/bar";
		assert_eq!(try_get_vcs_qual_base(i), Some("example.com/foo.git"));
		let i = "golang.org/x/mod";
		assert_eq!(try_get_vcs_qual_base(i), None);
	}

	#[test]
	fn test_parse_url() {
		let yaml_v3_resp = r#"
        <html>
            <head>
            <meta name="go-import" content="gopkg.in/yaml.v3 git https://gopkg.in/yaml.v3">
            </head>
            <body>
            go get gopkg.in/yaml.v3
            </body>
        </html>
"#;

		let y = GoModRequire::new("github.com/bytedance/sonic", "v1.13.1");
		let y_p = SingleTargetSeed::try_from(y).unwrap();
		assert_eq!(y_p.refspec, Some("v1.13.1".to_owned()));

		let SingleTargetSeedKind::RemoteRepo(r) = y_p.kind else {
			panic!()
		};
		assert_eq!(
			r.known_remote,
			Some(KnownRemote::GitHub {
				owner: "bytedance".to_owned(),
				repo: "sonic".to_owned()
			})
		);

		//  GoModRequire::new("gopkg.in/yaml.v3", "v3.0.1");
		//      We don't test this one directly because requires HTTP requests
		let opt_url_str = try_get_repo_url_from_go_get(yaml_v3_resp);
		assert_eq!(opt_url_str, Some("https://gopkg.in/yaml.v3"))
	}

	#[tokio::test]
	async fn test_resolve_package_lock_json() {
		let manifest = env!("CARGO_MANIFEST_DIR");
		let path_json: PathBuf = [manifest, "src", "target", "tests", "package-lock.json"]
			.iter()
			.collect();

		let mut target_seeds = resolve_package_lock_json(&path_json).await.unwrap();

		assert_eq!(target_seeds.len(), 3);

		let name = "chownr".to_string();
		let version = "2.0.0".to_string();
		let specifier = format!("{name} {version}");

		let chownr_seed = target_seeds.pop();
		assert_eq!(
			chownr_seed,
			Some(SingleTargetSeed {
				kind: SingleTargetSeedKind::Package(Package {
					purl: Url::parse("pkg:npm/chownr@2.0.0").unwrap(),
					name,
					version,
					host: PackageHost::Npm,
				}),
				refspec: None,
				specifier,
			})
		);
		_ = target_seeds.pop();

		let name = "@bugsounet/node-lpcm16".to_string();
		let version = "1.0.2".to_string();
		let specifier = format!("{name} {version}");

		let bugsounet_seed = target_seeds.pop();
		assert_eq!(
			bugsounet_seed,
			Some(SingleTargetSeed {
				kind: SingleTargetSeedKind::Package(Package {
					purl: Url::parse("pkg:npm/%40bugsounet/node-lpcm16@1.0.2").unwrap(),
					name,
					version,
					host: PackageHost::Npm,
				}),
				refspec: None,
				specifier,
			})
		);
	}
}
