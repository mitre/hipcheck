// SPDX-License-Identifier: Apache-2.0

use crate::task::manifest::{download_manifest::*, util};
use anyhow::Result;
use regex::Regex;
use serde::Deserialize;
use std::{collections::HashMap, sync::LazyLock};

// Objects parsed from the JSON returned by GitHub's Release API

#[derive(Clone, Debug, Deserialize)]
struct RawAssetDigest {
	pub name: String,
	pub size: usize,
	pub browser_download_url: url::Url,
}

#[derive(Clone, Debug, Deserialize)]
struct RawReleaseDigest {
	pub name: String,
	pub assets: Vec<RawAssetDigest>,
}

#[derive(Clone, Debug)]
pub struct ReleaseDigest(pub HashMap<String, Vec<DownloadManifestEntry>>);

// Used on the name of a RawReleaseDigest to extract release's name and version
static PLUGIN_RELEASE_REGEX: LazyLock<Regex> =
	LazyLock::new(|| Regex::new(r"(.*)-[v]?([0-9]+.[0-9]+.[0-9]+)").unwrap());

const SHA256_SUFFIX: &str = ".sha256";

impl TryFrom<Vec<RawReleaseDigest>> for ReleaseDigest {
	type Error = anyhow::Error;

	fn try_from(value: Vec<RawReleaseDigest>) -> Result<ReleaseDigest> {
		let mut releases = HashMap::<String, Vec<DownloadManifestEntry>>::new();

		// For each RawReleaseDigest,
		for release in value {
			// Attempt to split out the name and version string from the raw name,
			// this should fail for non-plugins because they follow a different release
			// naming convention
			let Some((name, version)) =
				PLUGIN_RELEASE_REGEX.captures(&release.name).and_then(|m| {
					parse_plugin_version(m.get(2).unwrap().as_str())
						.map(|v| (m.get(1).unwrap().as_str().to_owned(), v))
				})
			else {
				continue;
			};

			let plugin_entry: &mut _ = match releases.get_mut(&name) {
				Some(a) => a,
				None => {
					releases.insert(name.clone(), vec![]);
					releases.get_mut(&name).unwrap()
				}
			};

			// Create a lookup table of asset name to index in release.assets to
			// easily find the entry that a given .sha256 file is related to
			let asset_idxs: HashMap<String, usize> = HashMap::from_iter(
				release
					.assets
					.iter()
					.enumerate()
					.map(|(i, a)| (a.name.clone(), i)),
			);

			let name_prefix = format!("{name}-");

			// Look for files matching {name}-{arch}.{archive}.sha256
			for asset in release.assets.iter() {
				let Some(no_name) = asset.name.strip_prefix(&name_prefix) else {
					continue;
				};
				let Some(no_hash_ext) = no_name.strip_suffix(SHA256_SUFFIX) else {
					continue;
				};

				// The file related to this .sha256 file will be called `{name}-{arch}.{archive}`
				let tgt_name = asset.name.strip_suffix(SHA256_SUFFIX).unwrap().to_owned();
				let Some(idx) = asset_idxs.get(&tgt_name) else {
					eprintln!("warning: sha256 sum file found without corresponding asset");
					continue;
				};

				let tgt = release.assets.get(*idx).unwrap();

				// Store the URL from which we can get the sha256 if we need it later
				let digest = BufferedDigest::Remote(asset.browser_download_url.clone());

				let Some((arch, fmt_str)) = no_hash_ext.split_once('.') else {
					eprintln!("warning: malformed sha256 sum file name {}", asset.name);
					continue;
				};

				let Ok(format): Result<ArchiveFormat> = fmt_str.try_into() else {
					eprintln!("warning: malformed sha256 sum file name {}", asset.name);
					continue;
				};

				plugin_entry.push(DownloadManifestEntry {
					version: version.clone(),
					arch: Arch(arch.to_owned()),
					url: tgt.browser_download_url.clone(),
					size: Size::new(tgt.size as u64),
					hash: HashWithDigest {
						hash_algorithm: HashAlgorithm::Sha256,
						digest,
					},
					compress: Compress { format },
				});
			}
		}

		Ok(ReleaseDigest(releases))
	}
}

pub fn get_hipcheck_releases(github_api_token: &str) -> Result<ReleaseDigest> {
	let auth_agent = util::authenticated_agent::AuthenticatedAgent::new(github_api_token);
	let raw_rel_json = auth_agent
		.get("https://api.github.com/repos/mitre/hipcheck/releases")
		.call()?
		.into_string()?;
	let raw_digest: Vec<RawReleaseDigest> = serde_json::from_str(&raw_rel_json)?;
	raw_digest.try_into()
}
