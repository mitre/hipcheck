// SPDX-License-Identifier: Apache-2.0

//! Utilities for extracting repository info from SPDX documents.

use crate::{
	error::{Context as _, Result},
	hc_error,
};
use spdx_rs::models::SPDX;
use url::Url;

// The package download location field tag
const DLOAD_LOCN_TAG: &str = "PackageDownloadLocation";

// Tag-value delimiter
const DELIMITER: char = ':';

// Indicates that a download location does not exist
const DLOAD_NONE: &str = "NONE";

// Indicates that a download location may exist but is not present
const DLOAD_NOASSERT: &str = "NOASSERTION";

// Compatible download schemes
const SCM_GIT: &str = "git";
const SCM_GIT_PLUS: &str = "git+";
const SCM_HTTP: &str = "http";
const SCM_HTTPS: &str = "https";

/// Extract the first compatible package download location from an
/// SPDX document
pub fn extract_spdx_download_url(filepath: &str) -> Result<String> {
	let contents = std::fs::read_to_string(filepath)?;

	if contents.contains(DLOAD_LOCN_TAG) {
		extract_download_url_text(&contents)
	} else if let Ok(spdx) = serde_json::from_str(&contents) {
		extract_download_url_json(spdx)
	} else {
		Err(hc_error!("SPDX file is corrupt or incompatible"))
	}
}

// Extract the first compatible package download location from an SPDX
// object obtained from a JSON file
fn extract_download_url_json(spdx: SPDX) -> Result<String> {
	for package in spdx.package_information {
		if let Ok(url) = parse_download_url(&package.package_download_location) {
			return Ok(url);
		}
	}

	Err(hc_error!("No compatible download URLs found"))
}

// Extract the first compatible package download location from an SPDX
// text document
fn extract_download_url_text(contents: &str) -> Result<String> {
	for line in contents.lines() {
		if let Some((DLOAD_LOCN_TAG, value)) = line.split_once(DELIMITER) {
			match value.trim() {
				DLOAD_NONE | DLOAD_NOASSERT => (),
				locn => {
					if let Ok(url) = parse_download_url(locn) {
						return Ok(url);
					}
				}
			}
		}
	}

	Err(hc_error!("No compatible download URLs found"))
}

// Select and prepare compatible URIs and VCS locations for use
fn parse_download_url(locn: &str) -> Result<String> {
	let mut url = match locn.strip_prefix(SCM_GIT_PLUS) {
		Some(rest) => {
			// In this case, any secondary scheme is compatible
			Url::parse(rest).context("Invalid download location")
		}
		None => {
			let url = Url::parse(locn).context("Invalid download location")?;
			if matches!(url.scheme(), SCM_GIT | SCM_HTTP | SCM_HTTPS) {
				Ok(url)
			} else {
				Err(hc_error!("Package uses a non-Git VCS"))
			}
		}
	}?;

	// Remove element identifiers
	url.set_fragment(None);

	Ok(url.as_str().to_owned())
}
