#[allow(unused)]
pub mod types;
pub use types::*;

use clap::ValueEnum;
use packageurl::PackageUrl;
use serde::Serialize;
use std::str::FromStr;
use url::Url;

#[derive(Debug, Clone, ValueEnum, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetType {
	Maven,
	Npm,
	Pypi,
	Repo,
	Request,
	Spdx,
}

impl TargetType {
	/// Parses the target type if it is a pURL, GitHub repo, or SPDX file
	/// Updates the target string with the correct formatting if the original target string was a pURL
	pub fn try_resolve_from_target(tgt: &str) -> Option<(TargetType, String)> {
		use TargetType::*;

		// Check if the target is a pURL and parse it if it is
		if let Ok(purl) = PackageUrl::from_str(tgt) {
			match purl.ty() {
				"github" => {
					// Construct GitHub repo URL from pURL as the updated target string
					// For now we ignore the "version" field, which has GitHub tag information, until Hipcheck can cleanly handle things other than the main/master branch of a repo
					let mut url = "https://github.com/".to_string();
					// A repo must have an owner
					match purl.namespace() {
						Some(owner) => url.push_str(owner),
						None => return None,
					}
					url.push('/');
					let name = purl.name();
					url.push_str(name);
					url.push_str(".git");
					Some((Repo, url))
				}
				"maven" => {
					// Construct Maven package POM file URL from pURL as the updated target string

					// We currently only support parsing Maven packages hosted at repo1.maven.org
					let mut url = "https://repo1.maven.org/maven2/".to_string();
					// A package must belong to a group
					match purl.namespace() {
						Some(group) => url.push_str(&group.replace('.', "/")),
						None => return None,
					}
					url.push('/');
					let name = purl.name();
					url.push_str(name);
					// A package version is needed to construct a URL
					match purl.version() {
						Some(version) => {
							url.push('/');
							url.push_str(version);
							url.push('/');
							let pom_file = format!("{}-{}.pom", name, version);
							url.push_str(&pom_file);
						}
						None => return None,
					}
					Some((Maven, url))
				}
				"npm" => {
					// Construct NPM package w/ optional version from pURL as the updated target string
					let name = purl.name();
					let mut package = name.to_string();
					// Include version if provided
					if let Some(version) = purl.version() {
						package.push('@');
						package.push_str(version);
					}
					Some((Npm, package))
				}
				"pypi" => {
					// Construct PyPI package w/optional version from pURL as the updated target string
					let name = purl.name();
					let mut package = name.to_string();
					// Include version if providedc
					if let Some(version) = purl.version() {
						package.push('@');
						package.push_str(version);
					}
					Some((Pypi, package))
				}
				_ => None,
			}
		// Otherwise check if it is a Git VCS URL
		} else if tgt.starts_with("git+") {
			// Remove Git prefix
			let tgt_trimmed = tgt.replace("git+", "");
			// If the URL is not correctly formatted, we cannot identify the target type
			if let Ok(vcs_url) = Url::parse(&tgt_trimmed) {
				match vcs_url.scheme() {
					// If the URL is for a file, trim the file scheme idenfifier and return the presumptive file path
					// If the path is not valid, we will handle that error later
					"file" => {
						let filepath = vcs_url.path().to_string();
						Some((Repo, filepath))
					}
					// If the scheme is anything other than a file (e.g. https, ssh) clean up and return the repo URL
					_ => {
						// Remove any git ref information that trails the end of the URL
						let mut url =
							tgt_trimmed.split(".git").collect::<Vec<&str>>()[0].to_string();
						// Restore ".git" to the end of the URL, since we did not intend to remove that part
						url.push_str(".git");
						Some((Repo, url))
					}
				}
			} else {
				None
			}
		// Otherwise, check if it is a GitHub repo URL
		} else if tgt.starts_with("https://github.com/") {
			Some((Repo, tgt.to_string()))
		// Otherwise check if it has an SPDX file extension
		} else if tgt.ends_with(".spdx") {
			Some((Spdx, tgt.to_string()))
		} else {
			None
		}
	}
	pub fn as_str(&self) -> String {
		use serde_json::{to_value, Value};
		let Ok(Value::String(out)) = to_value(self) else {
			unreachable!();
		};
		out
	}
}
