// SPDX-License-Identifier: Apache-2.0

//! Utilities for extracting repository info from CycloneDX documents.

use super::{
	pm::{extract_repo_for_maven, extract_repo_for_npm, extract_repo_for_pypi},
	purl::parse_purl,
	TargetType,
};
use crate::{
	error::{Context as _, Result},
	hc_error,
};
use cyclonedx_bom::prelude::*;
use packageurl::PackageUrl;
use std::str::FromStr;
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
	println!("{purl}");

	match parse_purl(&purl) {
		Some((TargetType::Repo, url)) => Url::parse(&url).map_err(|e| hc_error!("Cannot parse constructed GitHub repository URL constructed from CycloneDX file download location: {}", e)),
		Some((TargetType::Maven, url)) => extract_repo_for_maven(&url).context("Could not get git repo URL for CycloneDX file's corresponding Maven package",),
		Some((TargetType::Npm, full_package)) =>  {
			let split_package: Vec<&str> = full_package.split('@').collect();
			let package = split_package[0];
			let version = match split_package.len() {
				1 => "no version",
				_ => split_package[1],
			};
			extract_repo_for_npm(package, version).context("Could not get git repo URL for CycloneDX file's corresponding NPM package",)
		},
		Some((TargetType::Pypi, full_package)) =>  {
			let split_package: Vec<&str> = full_package.split('@').collect();
			let package = split_package[0];
			let version = match split_package.len() {
				1 => "no version",
				_ => split_package[1],
			};
			extract_repo_for_pypi(package, version).context("Could not get git repo URL for CycloneDX file's corresponding PyPI package",)
		},
		_ => match purl.ty() {
			// It is possible to fail to resolve a URL from a GitHub repo or Maven package while parsing a pURL.
			// Give a special error message for those cases. Otherwise, assume we have parsed an incompatible pURL type.
			"github" => Err(hc_error!("Could not determine GitHub reposity download location from CycloneDX file.")),
			"maven" =>  Err(hc_error!("Could not determine git repo Url from CycloneDX file's corresponding Maven package.")),
			_ => Err(hc_error!("Download location for CycloneDX file is a pURL for a type not currently supported by Hipcheck.")),
		}
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
	fn test_extract_github_url_from_cyclonedx_json() {
		let manifest = env!("CARGO_MANIFEST_DIR");
		let path: PathBuf = [manifest, "src", "target", "tests", "juiceshop_bom.json"]
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
	fn test_extract_github_url_from_cyclonedx_xml() {
		let manifest = env!("CARGO_MANIFEST_DIR");
		let path: PathBuf = [manifest, "src", "target", "tests", "juiceshop_bom.xml"]
			.iter()
			.collect();
		let xml = path.to_str().unwrap();
		let url = extract_cyclonedx_download_url(xml).unwrap();
		assert_eq!(
			url.to_string(),
			"https://github.com/juice-shop/juice-shop.git".to_string()
		);
	}

	#[test]
	fn test_extract_npm_url_from_cyclonedx_json() {
		let manifest = env!("CARGO_MANIFEST_DIR");
		let path: PathBuf = [manifest, "src", "target", "tests", "cdxgen.cdx.json"]
			.iter()
			.collect();
		let json = path.to_str().unwrap();
		let url = extract_cyclonedx_download_url(json).unwrap();
		assert_eq!(
			url.to_string(),
			"https://github.com/CycloneDX/cdxgen.git".to_string()
		);
	}
}
