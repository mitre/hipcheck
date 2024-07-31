// SPDX-License-Identifier: Apache-2.0

//! Utilities for extracting repository info from CycloneDX documents.

use std::str::FromStr;

use crate::context::Context as _;
use crate::error::Result;
use crate::hc_error;
use crate::session::pm::{extract_repo_for_maven, extract_repo_for_npm, extract_repo_for_pypi};
use cyclonedx_bom::prelude::*;
use packageurl::PackageUrl;
use url::Url;

/// Extract the first compatible package download location from a
/// CycloneDX document
pub fn extract_cyclonedx_download_url(filepath: &str) -> Result<Url> {
	let contents = std::fs::read_to_string(filepath)?;

	if filepath.contains(".json") {
		let bom = Bom::parse_from_json(contents.as_bytes()).map_err(|_| {
			hc_error!("CycloneDX JSON file is corrupt or otherwise cannot be parsed. It may be in an incompatble CycloneDX format (only v. 1.3 - 1.5 supported)")
		})?;
		if bom.validate().passed() {
			extract_download_url(bom)
		} else {
			Err(hc_error!("CycloneDX file is not a valid SBOM"))
		}
	} else if filepath.contains(".xml") {
		let bom = parse_from_xml(contents)?;
		if bom.validate().passed() {
			extract_download_url(bom)
		} else {
			Err(hc_error!("CycloneDX file is not a valid SBOM"))
		}
	} else {
		Err(hc_error!("CycloneDX file is not in a comatible format"))
	}
}

// Extract the metadata component download location from a CycloneDX
// object obtained from a JSON or XML file
fn extract_download_url(bom: Bom) -> Result<Url> {
	let purl = PackageUrl::from_str(
        bom
        .metadata
        .ok_or(hc_error!("CycloneDX file is missing a metadata field. Download location cannot be extracted."))?
        .component
        .ok_or(hc_error!("CycloneDX file metadata missing a component field describing its own package. Download location cannot be extracted."))?
        .purl
        .ok_or(hc_error!("CycloneDX file metadata component information does not include a pURL. Download location cannot be extracted."))?
        .as_ref()
    )?;

	match purl.ty() {
		"github" => {
			// Get GitHub repo URL from pURL
			// For now we ignore the "version" field, which has GitHub tag information, until Hipcheck can cleanly handle things other than the main/master branch of a repo
			let mut url = "https://github.com/".to_string();
			// A repo must have an owner
			match purl.namespace() {
				Some(owner) => url.push_str(owner),
				None => {
					return Err(hc_error!(
					"Download location for CycloneDX file is a GitHub repository with no owner."
				))
				}
			}
			url.push('/');
			let name = purl.name();
			url.push_str(name);
			url.push_str(".git");

			Url::parse(&url).map_err(|e| hc_error!("Cannot parse constructed GitHub repository URL constructed from CycloneDX file download location: {}", e))
		}
		"maven" => {
			// First construct Maven package POM file URL from pURL as the updated target string
			// We currently only support parsing Maven packages hosted at repo1.maven.org
			let mut url = "https://repo1.maven.org/maven2/".to_string();
			// A package must belong to a group
			match purl.namespace() {
				Some(group) => url.push_str(&group.replace('.', "/")),
				None => {
					return Err(hc_error!(
						"Download location for CycloneDX file is a Maven package with no group."
					))
				}
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
				None => {
					return Err(hc_error!(
						"Download location for CycloneDX file is a Maven package with no version."
					))
				}
			}

			// Next attempt to get the git repo URL for the Maven package
			extract_repo_for_maven(&url).context(
				"Could not get git repo URL for CycloneDX file's corresponding Maven package",
			)
		}
		"npm" => {
			// First extract NPM package w/ optional version from the pURL
			let package = purl.name();
			let version = purl.version().unwrap_or("no version");

			// Next attempt to get the git repo URL for the NPM package
			extract_repo_for_npm(package, version).context(
				"Could not get git repo URL for CycloneDX file's corresponding NPM package",
			)
		}
		"pypi" => {
			// First extract PyPI package w/ optional version from the pURL
			let package = purl.name();
			let version = purl.version().unwrap_or("no version");

			// Next attempt to get the git repo URL for the PyPI package
			extract_repo_for_pypi(package, version).context(
				"Could not get git repo URL for CycloneDX file's corresponding PyPI package",
			)
		}
        _ => Err(hc_error!("Download location for CycloneDX file is a pURL for a type not currently supported by Hipcheck."))
	}
}

/// General function to parse an XML file; tries to parse as an XML of each compatible version in turn
fn parse_from_xml(contents: String) -> Result<Bom> {
	// First check if the XML file conforms to CycloneDX v. 1.5
	match Bom::parse_from_xml_v1_5(contents.as_bytes()) {
		Ok(bom) => Ok(bom),
		// If it does not,  check if the XML file conforms to CycloneDX v. 1.4
		_ => match Bom::parse_from_xml_v1_4(contents.as_bytes()) {
			Ok(bom) => Ok(bom),
			// If it does not,  check if the XML file conforms to CycloneDX v. 1.3. If not, return an error
			_ => Bom::parse_from_xml_v1_3(contents.as_bytes()).map_err(|_| {
				hc_error!("CycloneDX XML file is corrupt or otherwise cannot be parsed. It may be in an incompatble CycloneDX format (only v. 1.3 - 1.5 supported)")
			}),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::path::PathBuf;

	#[test]
	fn test_extract_url_from_cyclonedx_json() {
		let manifest = env!("CARGO_MANIFEST_DIR");
		let path: PathBuf = [manifest, "src", "session", "tests", "juiceshop_bom.json"]
			.iter()
			.collect();
		let json = path.to_str().unwrap();
		let url = extract_cyclonedx_download_url(json).unwrap();
		assert_eq!(
			url.to_string(),
			"https://github.com/juice-shop/juice-shop.git".to_string()
		);
	}

	#[test]
	fn test_extract_url_from_cyclonedx_xml() {
		let manifest = env!("CARGO_MANIFEST_DIR");
		let path: PathBuf = [manifest, "src", "session", "tests", "juiceshop_bom.xml"]
			.iter()
			.collect();
		let xml = path.to_str().unwrap();
		let url = extract_cyclonedx_download_url(xml).unwrap();
		assert_eq!(
			url.to_string(),
			"https://github.com/juice-shop/juice-shop.git".to_string()
		);
	}
}
