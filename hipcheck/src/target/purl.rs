use super::types::TargetType;
use packageurl::PackageUrl;

pub fn parse_purl(purl: &PackageUrl) -> Option<(TargetType, String)> {
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
			Some((TargetType::Repo, url))
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
			Some((TargetType::Maven, url))
		}
		"npm" => {
			// Construct NPM package w/ optional version from pURL as the updated target string
			let mut package = String::new();

			// Include scope if provided
			if let Some(scope) = purl.namespace() {
				package.push_str(scope);
				package.push('/');
			}
			let name = purl.name();
			package.push_str(name);
			// Include version if provided
			if let Some(version) = purl.version() {
				package.push('@');
				package.push_str(version);
			}
			Some((TargetType::Npm, package))
		}
		"pypi" => {
			// Construct PyPI package w/optional version from pURL as the updated target string
			let name = purl.name();
			let mut package = name.to_string();
			// Include version if provided
			if let Some(version) = purl.version() {
				package.push('@');
				package.push_str(version);
			}
			Some((TargetType::Pypi, package))
		}
		_ => None,
	}
}

#[cfg(test)]
mod tests {
	use std::str::FromStr;

	use super::*;
	use packageurl::PackageUrl;

	#[test]
	fn parse_github_purl() {
		let purl = PackageUrl::from_str("pkg:github/mitre/hipcheck").unwrap();
		let url = "https://github.com/mitre/hipcheck.git".to_string();
		let result = parse_purl(&purl).unwrap();
		assert_eq!(result, (TargetType::Repo, url));
	}

	#[test]
	fn parse_maven_purl() {
		let purl = PackageUrl::from_str("pkg:maven/joda-time/joda-time@2.10.10").unwrap();
		let url =
			"https://repo1.maven.org/maven2/joda-time/joda-time/2.10.10/joda-time-2.10.10.pom"
				.to_string();
		let result = parse_purl(&purl).unwrap();
		assert_eq!(result, (TargetType::Maven, url));
	}

	#[test]
	fn parse_pypi_purl() {
		let purl = PackageUrl::from_str("pkg:pypi/certifi@2021.5.30").unwrap();
		let package = "certifi@2021.5.30".to_string();
		let result = parse_purl(&purl).unwrap();
		assert_eq!(result, (TargetType::Pypi, package));
	}

	#[test]
	fn parse_npm_purl() {
		let purl = PackageUrl::from_str("pkg:npm/node-ipc@9.2.1").unwrap();
		let package = "node-ipc@9.2.1".to_string();
		let result = parse_purl(&purl).unwrap();
		assert_eq!(result, (TargetType::Npm, package));
	}
}
