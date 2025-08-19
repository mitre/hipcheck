// SPDX-License-Identifier: Apache-2.0

pub mod cyclone_dx;
mod multi;
pub mod pm;
pub mod purl;
pub mod resolve;
pub mod spdx;
pub mod types;
pub use types::*;

use crate::error::Error;

use clap::ValueEnum;
use packageurl::PackageUrl;
use purl::parse_purl;
use serde::Serialize;
use std::path::PathBuf;
use std::str::FromStr;
use url::Url;

pub trait ToTargetSeedKind {
	fn to_target_seed_kind(&self) -> Result<TargetSeedKind, Error>;
}

impl ToTargetSeedKind for SingleTargetSeedKind {
	fn to_target_seed_kind(&self) -> Result<TargetSeedKind, crate::error::Error> {
		Ok(TargetSeedKind::Single(self.clone()))
	}
}

impl ToTargetSeedKind for MultiTargetSeedKind {
	fn to_target_seed_kind(&self) -> Result<TargetSeedKind, crate::error::Error> {
		Ok(TargetSeedKind::Multi(self.clone()))
	}
}

pub trait ToTargetSeed {
	fn to_target_seed(&mut self) -> Result<TargetSeed, Error>;
}

impl ToTargetSeed for SingleTargetSeed {
	fn to_target_seed(&mut self) -> Result<TargetSeed, Error> {
		Ok(TargetSeed::Single(self.clone()))
	}
}

impl ToTargetSeed for MultiTargetSeed {
	fn to_target_seed(&mut self) -> Result<TargetSeed, Error> {
		Ok(TargetSeed::Multi(self.clone()))
	}
}

#[derive(Debug, Clone, PartialEq, Eq, ValueEnum, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetType {
	Maven,
	Npm,
	Pypi,
	Repo,
	Request,
	Sbom,
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
			// Remove Git prefix and check of the URL is correctly formatted
			let tgt_trimmed = tgt.replace("git+", "");
			// If the URL is not correctly formatted, we cannot identify the target type; otherwise we will parse the VCS URL information later
			match Url::parse(&tgt_trimmed) {
				Ok(_) => Some((Repo, tgt.to_string())),
				Err(_) => None,
			}
		// Otherwise, check if it a git protocol URL
		} else if tgt.starts_with("git://") {
			// Remove Git protocol prefix, and fetch Repo over http
			let tgt_trimmed = tgt.replace("git://", "https://");
			Some((Repo, tgt_trimmed.to_string()))
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
		use serde_json::{Value, to_value};
		let Ok(Value::String(out)) = to_value(self) else {
			unreachable!();
		};
		out
	}
}
