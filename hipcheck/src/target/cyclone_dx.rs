// SPDX-License-Identifier: Apache-2.0

//! Utilities for extracting repository info from CycloneDX documents.

use super::{TargetType, pm::extract_repo_for_maven, purl::parse_purl};
use crate::{
	error::{Context as _, Result},
	hc_error,
	target::types::*,
};
use cyclonedx_bom::prelude::*;
use packageurl::PackageUrl;
use std::str::FromStr;
use url::Url;

/// return type options for extract_download_url. The target type can be a Url or Package
#[derive(Debug)]
pub enum BomTarget {
	Url(Url),
	Package(Package),
}

/// Extract the first compatible package download location from a
/// CycloneDX document.
pub fn extract_cyclonedx_download_url(filepath: &str) -> Result<BomTarget> {
	let contents = std::fs::read_to_string(filepath)?;

	if filepath.contains(".json") {
		let bom = Bom::parse_from_json(contents.as_bytes()).map_err(|_| {
			hc_error!("CycloneDX JSON file is corrupt or otherwise cannot be parsed. It may be in an incompatble CycloneDX format (only v. 1.3 - 1.5 supported)")
		})?;
		if bom.validate().passed() {
			extract_download_url(bom)
				.with_context(|| format!("Failed to extract download URL from: {}", filepath))
		} else {
			Err(hc_error!("CycloneDX file is not a valid SBOM"))
		}
	} else if filepath.contains(".xml") {
		let bom = parse_from_xml(contents)?;
		if bom.validate().passed() {
			extract_download_url(bom)
				.with_context(|| format!("Failed to extract download URL from: {}", filepath))
		} else {
			Err(hc_error!("CycloneDX file is not a valid SBOM"))
		}
	} else {
		Err(hc_error!("CycloneDX file is not in a compatible format"))
	}
}

/// Extracts the purl from a BOM file.
fn extract_purl(bom: &Bom) -> Result<PackageUrl<'_>> {
	let purl = PackageUrl::from_str(
        bom
        .metadata.clone()
        .ok_or(hc_error!("CycloneDX file is missing a metadata field. Download location cannot be extracted."))?
        .component
        .ok_or(hc_error!("CycloneDX file metadata missing a component field describing its own package. Download location cannot be extracted."))?
        .purl
        .ok_or(hc_error!("CycloneDX file metadata component information does not include a pURL. Download location cannot be extracted."))?
        .as_ref()
    )?;
	Ok(purl)
}

/// Returns the target type from the BOM file's purl.
/// If the purl's type is github or maven, return a repo URL.
/// If the target type is npm or pypi, return the package
fn extract_download_url(bom: Bom) -> Result<BomTarget> {
	let purl = extract_purl(&bom)?;
	match parse_purl(&purl) {
		Some((TargetType::Repo, url)) => {
			let url = Url::parse(&url).map_err(|e| hc_error!("Cannot parse constructed GitHub repository URL constructed from CycloneDX file download location: {}", e))?;
			Ok(BomTarget::Url(url))
		}
		Some((TargetType::Maven, url)) => {
			let url = extract_repo_for_maven(&url).context(
				"Could not get git repo URL for CycloneDX file's corresponding Maven package",
			)?;
			Ok(BomTarget::Url(url))
		}
		Some((TargetType::Npm, full_package)) => {
			let split_package: Vec<&str> = full_package.split('@').collect();
			let name = split_package[0].to_string();
			let version = match split_package.len() {
				1 => "no version",
				_ => split_package[1],
			}
			.to_string();
			// converts the purl to a url so it can be passed to a package
			let purl_as_url = Url::parse(&purl.to_string())
				.map_err(|e| hc_error!("Cannot convert the purl to a url: {}", e))?;
			let package = Package {
				purl: purl_as_url,
				name,
				version,
				host: PackageHost::Npm,
			};
			Ok(BomTarget::Package(package))
		}
		Some((TargetType::Pypi, full_package)) => {
			let split_package: Vec<&str> = full_package.split('@').collect();
			let version = match split_package.len() {
				1 => "no version",
				_ => split_package[1],
			}
			.to_string();
			let first_half_of_package: Vec<&str> = split_package[0].split(":").collect();
			let name = first_half_of_package[1].to_string();
			// the purl must be in url format in order to pass to a package
			let purl_as_url = Url::parse(&purl.to_string())
				.map_err(|e| hc_error!("Cannot convert the purl to a url: {}", e))?;
			let package = Package {
				purl: purl_as_url,
				name,
				version,
				host: PackageHost::PyPI,
			};
			Ok(BomTarget::Package(package))
		}
		_ => {
			match purl.ty() {
				// It is possible to fail to resolve a URL from a GitHub repo or Maven package while parsing a pURL.
				// Give a special error message for those cases. Otherwise, assume we have parsed an incompatible pURL type.
				"github" => Err(hc_error!(
					"Could not determine GitHub reposity download location from CycloneDX file."
				)),
				"maven" => Err(hc_error!(
					"Could not determine git repo Url from CycloneDX file's corresponding Maven package."
				)),
				_ => Err(hc_error!(
					"Download location for CycloneDX file is a pURL for a type not currently supported by Hipcheck."
				)),
			}
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
	/// Tests for extracting the purl
	#[test]
	fn test_extract_purl_from_cyclonedx_juiceshop_json() {
		let manifest = env!("CARGO_MANIFEST_DIR");
		let path: PathBuf = [manifest, "src", "target", "tests", "juiceshop_bom.json"]
			.iter()
			.collect();
		let json_path = path.to_str().unwrap();
		let contents = std::fs::read_to_string(json_path).unwrap();
		let bom = Bom::parse_from_json(contents.as_bytes()).map_err(|_| {
			hc_error!("CycloneDX JSON file is corrupt or otherwise cannot be parsed. It may be in an incompatble CycloneDX format (only v. 1.3 - 1.5 supported)")
		}).unwrap();
		let purl = extract_purl(&bom).unwrap();
		assert_eq!(
			purl.to_string(),
			"pkg:github/juice-shop/juice-shop".to_string()
		);
	}
	#[test]
	fn test_extract_purl_from_cyclonedx_bats_cdx_json() {
		let manifest = env!("CARGO_MANIFEST_DIR");
		let path: PathBuf = [manifest, "src", "target", "tests", "bats.cdx.json"]
			.iter()
			.collect();
		let json_path = path.to_str().unwrap();
		let contents = std::fs::read_to_string(json_path).unwrap();
		let bom = Bom::parse_from_json(contents.as_bytes()).map_err(|_| {
			hc_error!("CycloneDX JSON file is corrupt or otherwise cannot be parsed. It may be in an incompatble CycloneDX format (only v. 1.3 - 1.5 supported)")
		}).unwrap();
		let purl = extract_purl(&bom).unwrap();
		assert_eq!(purl.to_string(), "pkg:npm/bats@1.9.0".to_string());
	}
	#[test]
	fn test_extract_purl_from_cyclonedx_xml() {
		let manifest = env!("CARGO_MANIFEST_DIR");
		let path: PathBuf = [manifest, "src", "target", "tests", "juiceshop_bom.xml"]
			.iter()
			.collect();
		let xml_path = path.to_str().unwrap();
		let contents = std::fs::read_to_string(xml_path).unwrap();
		let bom = Bom::parse_from_xml_with_version(contents.as_bytes(), SpecVersion::V1_4).map_err(|_| {
			hc_error!("CycloneDX XML file is corrupt or otherwise cannot be parsed. It may be in an incompatble CycloneDX format (only v. 1.3 - 1.5 supported)")
		}).unwrap();
		let purl = extract_purl(&bom).unwrap();
		assert_eq!(
			purl.to_string(),
			"pkg:github/juice-shop/juice-shop".to_string()
		);
	}
	/// Tests for getting the target type
	#[test]
	fn test_extract_github_url_from_cyclonedx_json() {
		let manifest = env!("CARGO_MANIFEST_DIR");
		let path: PathBuf = [manifest, "src", "target", "tests", "juiceshop_bom.json"]
			.iter()
			.collect();
		let json = path.to_str().unwrap();
		let result = extract_cyclonedx_download_url(json).unwrap();
		match result {
			BomTarget::Url(url) => assert_eq!(
				"https://github.com/juice-shop/juice-shop.git",
				url.to_string()
			),

			BomTarget::Package(_package) => panic!("Error, returns package instead of url"),
		}
	}

	#[test]
	fn test_extract_github_url_from_cyclonedx_xml() {
		let manifest = env!("CARGO_MANIFEST_DIR");
		let path: PathBuf = [manifest, "src", "target", "tests", "juiceshop_bom.xml"]
			.iter()
			.collect();
		let xml = path.to_str().unwrap();
		let return_value = extract_cyclonedx_download_url(xml).unwrap();
		match return_value {
			BomTarget::Url(url) => assert_eq!(
				"https://github.com/juice-shop/juice-shop.git",
				url.to_string()
			),

			BomTarget::Package(_package) => panic!("Error, returns package instead of url"),
		}
	}

	#[test]
	fn test_extract_npm_package_from_cyclonedx_json() {
		let manifest = env!("CARGO_MANIFEST_DIR");
		let path: PathBuf = [manifest, "src", "target", "tests", "bats.cdx.json"]
			.iter()
			.collect();
		let json = path.to_str().unwrap();
		let package = extract_cyclonedx_download_url(json).unwrap();
		match package {
			BomTarget::Url(_url) => panic!("Error, returns url instead of package"),
			BomTarget::Package(package) => {
				assert_eq!("pkg:npm/bats@1.9.0", package.purl.to_string());
				assert_eq!("bats", package.name);
				assert_eq!("1.9.0", package.version);
				assert_eq!(PackageHost::Npm, package.host);
			}
		}
	}
}
