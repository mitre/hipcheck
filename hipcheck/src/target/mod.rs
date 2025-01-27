// SPDX-License-Identifier: Apache-2.0

pub mod cyclone_dx;
pub mod pm;
pub mod purl;
pub mod resolve;
pub mod spdx;
pub mod types;
use purl::parse_purl;
pub use types::*;

use crate::error::Error;

use packageurl::PackageUrl;
use serde::Serialize;
use std::path::PathBuf;
use std::str::FromStr;
use url::Url;

pub trait ToTargetSeedKind {
	fn to_target_seed_kind(&self) -> Result<TargetSeedKind, Error>;
}

pub trait ToTargetSeed {
	fn to_target_seed(&self) -> Result<TargetSeed, Error>;
}

impl TargetType {
	/// Parses the target type if it is a pURL, GitHub repo, or SPDX file
	/// Updates the target string with the correct formatting if the original target string was a pURL
	pub fn try_resolve_from_target(tgt: &str) -> Option<(TargetType, String)> {
		use TargetType::*;

		// Check if the target is a pURL and parse it if it is
		if let Ok(purl) = PackageUrl::from_str(tgt) {
			parse_purl(&purl)
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
		// Otherwise check if it has an SPDX or CycloneDX SBOM file extension
		} else if tgt.ends_with(".spdx")
			|| tgt.ends_with("bom.json")
			|| tgt.ends_with(".cdx.json")
			|| tgt.ends_with("bom.xml")
			|| tgt.ends_with(".cdx.xml")
		{
			Some((Sbom, tgt.to_string()))
		// If is path to a file/dir that exists, treat as a local Repo
		} else if PathBuf::from(tgt).exists() {
			Some((Repo, tgt.to_string()))
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
