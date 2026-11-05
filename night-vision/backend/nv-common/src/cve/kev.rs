//! KEV-linked vulnerability matching for reachable NPM package versions.

use crate::db::entities::{cisa_kev_entries, cve_list_records, package_sources};
use crate::npm_semver::{NpmVersion as NpmRangeVersion, parse_range};
use percent_encoding::percent_decode_str;
use sea_orm::{ColumnTrait as _, DatabaseConnection, EntityTrait as _, QueryFilter as _};
use semver::Version;
use serde_json::Value;
use std::collections::HashMap;

/// A reachable concrete NPM package version from a submitted package source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReachableNpmPackageVersion {
	pub package_name: String,
	pub version: String,
	pub source_evidence: String,
}

/// KEV metadata preserved with a match or explainable unknown result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KevContext {
	pub cve_id: String,
	pub vendor_project: Option<String>,
	pub product: Option<String>,
	pub vulnerability_name: Option<String>,
	pub date_added: Option<String>,
}

/// Confidence in a KEV-to-package-version association.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KevNpmMatchConfidence {
	High,
	Unknown,
}

/// Whether a KEV-linked CVE could be matched to a reachable NPM package version.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum KevNpmMatchStatus {
	Affected,
	Unknown,
}

/// A KEV-linked CVE assessment for one submitted source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KevAffectedNpmPackageVersion {
	pub cve_id: String,
	pub kev_context: KevContext,
	pub package_name: Option<String>,
	pub affected_version: Option<String>,
	pub source_evidence: Vec<String>,
	pub confidence: KevNpmMatchConfidence,
	pub caveats: Vec<String>,
	pub status: KevNpmMatchStatus,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NpmAffectedPackage {
	package_name: Option<String>,
	ranges: Vec<AffectedVersionRange>,
	evidence: Vec<String>,
	caveats: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AffectedVersionRange {
	introduced: Option<String>,
	fixed: Option<String>,
	last_affected: Option<String>,
	exact: Option<String>,
	npm_range: Option<String>,
	evidence: String,
}

/// Find reachable NPM package versions in a submitted source that are affected by KEV-linked CVEs.
///
/// This reads concrete versions from NPM lockfile-shaped JSON in `package_sources.file_contents`.
/// `package.json` ranges are not treated as concrete reachable versions.
pub async fn kev_affected_npm_package_versions_for_source(
	db: &DatabaseConnection,
	source_id: &str,
) -> Result<Vec<KevAffectedNpmPackageVersion>, KevNpmMatchError> {
	let Some(source) = package_sources::Entity::find()
		.filter(package_sources::Column::SourceId.eq(source_id))
		.one(db)
		.await
		.map_err(KevNpmMatchError::Db)?
	else {
		return Ok(Vec::new());
	};

	let reachable = reachable_npm_package_versions_from_source(&source.file_contents)
		.map_err(KevNpmMatchError::PackageSourceJson)?;
	kev_affected_npm_package_versions(db, &reachable).await
}

/// Match KEV-linked CVE records against caller-provided reachable NPM package versions.
pub async fn kev_affected_npm_package_versions(
	db: &DatabaseConnection,
	reachable: &[ReachableNpmPackageVersion],
) -> Result<Vec<KevAffectedNpmPackageVersion>, KevNpmMatchError> {
	let kev_entries = cisa_kev_entries::Entity::find()
		.filter(cisa_kev_entries::Column::RemovedAt.is_null())
		.all(db)
		.await
		.map_err(KevNpmMatchError::Db)?;
	let cve_records = cve_list_records::Entity::find()
		.filter(
			cve_list_records::Column::CveId
				.is_in(kev_entries.iter().map(|entry| entry.cve_id.clone())),
		)
		.filter(cve_list_records::Column::Deleted.eq(false))
		.all(db)
		.await
		.map_err(KevNpmMatchError::Db)?
		.into_iter()
		.map(|record| (record.cve_id.clone(), record))
		.collect::<HashMap<_, _>>();
	let mut matches = Vec::new();

	for kev in kev_entries {
		let kev_context = kev_context_from_entry(&kev.cve_id, &kev.entry);
		let Some(cve_record) = cve_records.get(&kev.cve_id) else {
			matches.push(unknown_without_cve_enrichment(kev_context));
			continue;
		};

		matches.extend(match_kev_cve_record_to_reachable_npm(
			kev_context,
			&cve_record.record,
			reachable,
		));
	}

	Ok(matches)
}

/// Match KEV-linked CVEs against one concrete NPM package version.
///
/// Unknown results without a package identity describe a catalog-enrichment
/// gap, not an association with the requested package version, so they are
/// excluded from this single-version view.
pub async fn kev_affected_npm_package_version(
	db: &DatabaseConnection,
	package: ReachableNpmPackageVersion,
) -> Result<Vec<KevAffectedNpmPackageVersion>, KevNpmMatchError> {
	let package_name = package.package_name.clone();
	kev_affected_npm_package_versions(db, &[package])
		.await
		.map(|matches| {
			let mut matches = matches
				.into_iter()
				.filter(|matched| {
					matched.status == KevNpmMatchStatus::Affected
						|| matched.package_name.as_deref() == Some(package_name.as_str())
				})
				.collect::<Vec<_>>();
			matches.sort_by(|left, right| {
				left.cve_id
					.cmp(&right.cve_id)
					.then(left.affected_version.cmp(&right.affected_version))
			});
			matches
		})
}

fn match_kev_cve_record_to_reachable_npm(
	kev_context: KevContext,
	cve_record: &Value,
	reachable: &[ReachableNpmPackageVersion],
) -> Vec<KevAffectedNpmPackageVersion> {
	let affected_packages = npm_affected_packages_from_cve(&kev_context, cve_record);
	if affected_packages.is_empty() {
		return vec![unknown(
			kev_context,
			None,
			vec!["CVE record has no NPM affected package metadata".to_owned()],
			Vec::new(),
		)];
	}

	let mut results = Vec::new();
	for affected in affected_packages {
		let Some(package_name) = affected.package_name.clone() else {
			results.push(unknown(
				kev_context.clone(),
				None,
				affected.evidence,
				caveats_with(
					&affected.caveats,
					"affected package name is missing or ambiguous",
				),
			));
			continue;
		};

		if affected.ranges.is_empty() {
			results.push(unknown(
				kev_context.clone(),
				Some(package_name),
				affected.evidence,
				caveats_with(&affected.caveats, "affected version range is missing"),
			));
			continue;
		}

		for version in reachable
			.iter()
			.filter(|version| version.package_name == package_name)
		{
			match affected_ranges_include_version(&affected.ranges, &version.version) {
				RangeMatch::Matched(evidence) => {
					let mut source_evidence = affected.evidence.clone();
					source_evidence.push(version.source_evidence.clone());
					source_evidence.push(evidence);
					results.push(KevAffectedNpmPackageVersion {
						cve_id: kev_context.cve_id.clone(),
						kev_context: kev_context.clone(),
						package_name: Some(package_name.clone()),
						affected_version: Some(version.version.clone()),
						source_evidence,
						confidence: KevNpmMatchConfidence::High,
						caveats: affected.caveats.clone(),
						status: KevNpmMatchStatus::Affected,
					});
				}
				RangeMatch::NoMatch => {}
				RangeMatch::Unknown(caveat) => results.push(unknown(
					kev_context.clone(),
					Some(package_name.clone()),
					affected.evidence.clone(),
					caveats_with(&affected.caveats, caveat),
				)),
			}
		}
	}

	results
}

fn npm_affected_packages_from_cve(
	kev_context: &KevContext,
	cve_record: &Value,
) -> Vec<NpmAffectedPackage> {
	cve_record
		.pointer("/containers/cna")
		.map(|container| affected_from_container(kev_context, container, "containers.cna"))
		.unwrap_or_default()
}

fn affected_from_container(
	kev_context: &KevContext,
	container: &Value,
	base_path: &str,
) -> Vec<NpmAffectedPackage> {
	container
		.get("affected")
		.and_then(Value::as_array)
		.map(|affected| {
			affected
				.iter()
				.enumerate()
				.filter_map(|(index, entry)| {
					npm_affected_package(
						kev_context,
						entry,
						&format!("{base_path}.affected[{index}]"),
					)
				})
				.collect()
		})
		.unwrap_or_default()
}

fn npm_affected_package(
	kev_context: &KevContext,
	entry: &Value,
	path: &str,
) -> Option<NpmAffectedPackage> {
	if !affected_entry_is_npm(kev_context, entry) {
		return None;
	}

	let package_name = package_name_from_affected_entry(kev_context, entry);
	let mut caveats = Vec::new();
	if package_name.is_none() {
		caveats.push(
			"NPM ecosystem evidence was present but package identity could not be derived"
				.to_owned(),
		);
	}

	let ranges = affected_ranges_from_entry(entry, path, &mut caveats);
	Some(NpmAffectedPackage {
		package_name,
		ranges,
		evidence: vec![format!("{path} declares NPM affected metadata")],
		caveats,
	})
}

fn affected_entry_is_npm(kev_context: &KevContext, entry: &Value) -> bool {
	package_name_from_package_url(entry).is_some()
		|| collection_url_is_npm(entry)
		|| kev_context.vendor_project.as_deref() == Some("Npm package")
}

fn collection_url_is_npm(entry: &Value) -> bool {
	entry
		.get("collectionURL")
		.and_then(Value::as_str)
		.is_some_and(|value| {
			matches!(
				value.trim_end_matches('/'),
				"https://www.npmjs.com" | "https://registry.npmjs.org"
			)
		})
}

fn package_name_from_affected_entry(kev_context: &KevContext, entry: &Value) -> Option<String> {
	if let Some(package_name) = package_name_from_package_url(entry) {
		return Some(package_name);
	}

	let package_name = entry
		.get("packageName")
		.and_then(Value::as_str)
		.or_else(|| {
			if kev_context.vendor_project.as_deref() == Some("Npm package") {
				entry.get("product").and_then(Value::as_str)
			} else {
				None
			}
		})?;
	npm_package_name_from_string(package_name)
}

fn package_name_from_package_url(entry: &Value) -> Option<String> {
	let package_url = entry.get("packageURL").and_then(Value::as_str)?;
	let package_name = package_url.strip_prefix("pkg:npm/")?;
	let package_name = percent_decode_str(package_name).decode_utf8().ok()?;
	npm_package_name_from_string(&package_name)
}

fn npm_package_name_from_string(package_name: &str) -> Option<String> {
	let package_name = package_name
		.strip_prefix("npm:")
		.or_else(|| package_name.strip_prefix("pkg:npm/"))
		.unwrap_or(package_name);
	strip_npm_package_version_suffix(package_name)
		.filter(|name| !name.trim().is_empty())
		.map(str::to_owned)
}

fn strip_npm_package_version_suffix(package_name: &str) -> Option<&str> {
	let Some((name, _version)) = package_name.rsplit_once('@') else {
		return Some(package_name);
	};

	if name.is_empty() {
		Some(package_name)
	} else {
		Some(name)
	}
}

fn affected_ranges_from_entry(
	entry: &Value,
	path: &str,
	caveats: &mut Vec<String>,
) -> Vec<AffectedVersionRange> {
	let Some(versions) = entry.get("versions").and_then(Value::as_array) else {
		caveats.push("CVE affected entry has no versions array".to_owned());
		return Vec::new();
	};

	let mut ranges = Vec::new();
	let mut introduced = None;
	for (index, version) in versions.iter().enumerate() {
		let status = version.get("status").and_then(Value::as_str);
		let version_value = version
			.get("version")
			.and_then(Value::as_str)
			.map(str::to_owned);
		match status {
			Some("affected") => {
				if version.get("lessThan").is_some() || version.get("lessThanOrEqual").is_some() {
					ranges.push(AffectedVersionRange {
						introduced: version_value,
						fixed: version
							.get("lessThan")
							.and_then(Value::as_str)
							.map(str::to_owned),
						last_affected: version
							.get("lessThanOrEqual")
							.and_then(Value::as_str)
							.map(str::to_owned),
						exact: None,
						npm_range: None,
						evidence: format!("{path}.versions[{index}] affected range"),
					});
				} else if let Some(exact) = version_value {
					ranges.push(AffectedVersionRange {
						introduced: None,
						fixed: None,
						last_affected: None,
						exact: None,
						npm_range: Some(exact),
						evidence: format!("{path}.versions[{index}] affected NPM range"),
					});
				} else {
					caveats.push(format!(
						"{path}.versions[{index}] is affected but has no version bounds"
					));
				}
			}
			Some("unaffected") => {
				if let Some(fixed) = version_value
					&& let Some(start) = introduced.take()
				{
					ranges.push(AffectedVersionRange {
						introduced: Some(start),
						fixed: Some(fixed),
						last_affected: None,
						exact: None,
						npm_range: None,
						evidence: format!("{path}.versions[{index}] closes affected range"),
					});
				}
			}
			_ => {
				if let Some(version_value) = version_value {
					introduced = Some(version_value);
				}
			}
		}
	}

	ranges
}

enum RangeMatch {
	Matched(String),
	NoMatch,
	Unknown(&'static str),
}

fn affected_ranges_include_version(ranges: &[AffectedVersionRange], version: &str) -> RangeMatch {
	let Ok(version) = Version::parse(version) else {
		return RangeMatch::Unknown("reachable package version is not valid SemVer");
	};

	for range in ranges {
		if let Some(npm_range) = &range.npm_range {
			let Ok(npm_range) = parse_range(npm_range) else {
				return RangeMatch::Unknown("CVE affected NPM range is not valid SemVer");
			};
			let Ok(npm_version) = NpmRangeVersion::parse(version.to_string()) else {
				return RangeMatch::Unknown("reachable package version is not valid SemVer");
			};
			if npm_range.satisfies(&npm_version) {
				return RangeMatch::Matched(range.evidence.clone());
			}
			continue;
		}

		if let Some(exact) = &range.exact {
			let Ok(exact) = Version::parse(exact) else {
				return RangeMatch::Unknown("CVE affected exact version is not valid SemVer");
			};
			if version == exact {
				return RangeMatch::Matched(range.evidence.clone());
			}
			continue;
		}

		if let Some(introduced) = &range.introduced {
			let Ok(introduced) = Version::parse(introduced) else {
				return RangeMatch::Unknown("CVE affected introduced version is not valid SemVer");
			};
			if version < introduced {
				continue;
			}
		}

		if let Some(fixed) = &range.fixed {
			let Ok(fixed) = Version::parse(fixed) else {
				return RangeMatch::Unknown("CVE affected upper bound is not valid SemVer");
			};
			if version >= fixed {
				continue;
			}
		}

		if let Some(last_affected) = &range.last_affected {
			let Ok(last_affected) = Version::parse(last_affected) else {
				return RangeMatch::Unknown("CVE affected upper bound is not valid SemVer");
			};
			if version > last_affected {
				continue;
			}
		}

		return RangeMatch::Matched(range.evidence.clone());
	}

	RangeMatch::NoMatch
}

fn reachable_npm_package_versions_from_source(
	file_contents: &str,
) -> Result<Vec<ReachableNpmPackageVersion>, serde_json::Error> {
	let source: Value = serde_json::from_str(file_contents)?;
	let mut versions = Vec::new();

	if let Some(packages) = source.get("packages").and_then(Value::as_object) {
		for (path, package) in packages {
			let Some(name) = npm_package_name_from_lockfile_path(path) else {
				continue;
			};
			let Some(version) = package.get("version").and_then(Value::as_str) else {
				continue;
			};
			versions.push(ReachableNpmPackageVersion {
				package_name: name.clone(),
				version: version.to_owned(),
				source_evidence: format!("package-lock packages.{path} resolves {name}@{version}"),
			});
		}
	}

	if let Some(dependencies) = source.get("dependencies").and_then(Value::as_object) {
		collect_lockfile_dependencies(dependencies, &mut versions);
	}

	versions.sort_by(|a, b| {
		a.package_name
			.cmp(&b.package_name)
			.then(a.version.cmp(&b.version))
	});
	versions.dedup_by(|a, b| a.package_name == b.package_name && a.version == b.version);
	Ok(versions)
}

fn npm_package_name_from_lockfile_path(path: &str) -> Option<String> {
	let package_name = path.rsplit_once("node_modules/")?.1;
	let valid_unscoped_name = !package_name.is_empty() && !package_name.contains('/');
	let valid_scoped_name = package_name.strip_prefix('@').is_some_and(|name| {
		name.split_once('/').is_some_and(|(scope, name)| {
			!scope.is_empty() && !name.is_empty() && !name.contains('/')
		})
	});

	(valid_unscoped_name || valid_scoped_name).then(|| package_name.to_owned())
}

fn collect_lockfile_dependencies(
	dependencies: &serde_json::Map<String, Value>,
	versions: &mut Vec<ReachableNpmPackageVersion>,
) {
	for (name, dependency) in dependencies {
		if let Some(version) = dependency.get("version").and_then(Value::as_str) {
			versions.push(ReachableNpmPackageVersion {
				package_name: name.clone(),
				version: version.to_owned(),
				source_evidence: format!(
					"package-lock dependencies.{name}.version resolves {name}@{version}"
				),
			});
		}

		if let Some(child_dependencies) = dependency.get("dependencies").and_then(Value::as_object)
		{
			collect_lockfile_dependencies(child_dependencies, versions);
		}
	}
}

fn kev_context_from_entry(cve_id: &str, entry: &Value) -> KevContext {
	KevContext {
		cve_id: cve_id.to_owned(),
		vendor_project: kev_string(entry, "vendorProject"),
		product: kev_string(entry, "product"),
		vulnerability_name: kev_string(entry, "vulnerabilityName"),
		date_added: kev_string(entry, "dateAdded"),
	}
}

fn kev_string(entry: &Value, field: &str) -> Option<String> {
	entry.get(field).and_then(Value::as_str).map(str::to_owned)
}

fn unknown_without_cve_enrichment(kev_context: KevContext) -> KevAffectedNpmPackageVersion {
	unknown(
		kev_context,
		None,
		vec!["KEV entry has no usable active CVE List enrichment record".to_owned()],
		Vec::new(),
	)
}

fn unknown(
	kev_context: KevContext,
	package_name: Option<String>,
	source_evidence: Vec<String>,
	caveats: Vec<String>,
) -> KevAffectedNpmPackageVersion {
	KevAffectedNpmPackageVersion {
		cve_id: kev_context.cve_id.clone(),
		kev_context,
		package_name,
		affected_version: None,
		source_evidence,
		confidence: KevNpmMatchConfidence::Unknown,
		caveats,
		status: KevNpmMatchStatus::Unknown,
	}
}

fn caveats_with(caveats: &[String], caveat: impl Into<String>) -> Vec<String> {
	let mut next = caveats.to_vec();
	next.push(caveat.into());
	next
}

/// Failure while matching KEV-linked CVEs to reachable NPM package versions.
#[derive(Debug)]
pub enum KevNpmMatchError {
	Db(sea_orm::DbErr),
	PackageSourceJson(serde_json::Error),
}

impl std::fmt::Display for KevNpmMatchError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Db(_) => write!(f, "failed to read KEV/CVE/package-source data"),
			Self::PackageSourceJson(_) => {
				write!(f, "failed to parse submitted package source JSON")
			}
		}
	}
}

impl std::error::Error for KevNpmMatchError {
	fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
		match self {
			Self::Db(err) => Some(err),
			Self::PackageSourceJson(err) => Some(err),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sea_orm::{DbBackend, MockDatabase};
	use serde_json::json;

	fn kev_context(cve_id: &str) -> KevContext {
		KevContext {
			cve_id: cve_id.to_owned(),
			vendor_project: Some("Example".to_owned()),
			product: Some("Example Product".to_owned()),
			vulnerability_name: Some("Example Vulnerability".to_owned()),
			date_added: Some("2026-08-03".to_owned()),
		}
	}

	fn npm_kev_context(cve_id: &str) -> KevContext {
		KevContext {
			vendor_project: Some("Npm package".to_owned()),
			..kev_context(cve_id)
		}
	}

	fn reachable(package_name: &str, version: &str) -> Vec<ReachableNpmPackageVersion> {
		vec![ReachableNpmPackageVersion {
			package_name: package_name.to_owned(),
			version: version.to_owned(),
			source_evidence: format!("test source resolves {package_name}@{version}"),
		}]
	}

	fn cve(package_name: Value, versions: Value) -> Value {
		json!({
			"containers": {
				"cna": {
					"affected": [{
						"collectionURL": "https://www.npmjs.com/",
						"packageName": package_name,
						"versions": versions
					}]
				}
			}
		})
	}

	#[test]
	fn direct_match_returns_kev_package_version_and_evidence() {
		let record = cve(
			json!("left-pad"),
			json!([{ "status": "affected", "version": "1.0.0", "lessThan": "1.2.0" }]),
		);

		let matches = match_kev_cve_record_to_reachable_npm(
			kev_context("CVE-2026-1000"),
			&record,
			&reachable("left-pad", "1.1.0"),
		);

		assert_eq!(matches.len(), 1);
		let matched = &matches[0];
		assert_eq!(matched.status, KevNpmMatchStatus::Affected);
		assert_eq!(matched.cve_id, "CVE-2026-1000");
		assert_eq!(matched.package_name.as_deref(), Some("left-pad"));
		assert_eq!(matched.affected_version.as_deref(), Some("1.1.0"));
		assert_eq!(matched.confidence, KevNpmMatchConfidence::High);
		assert!(
			matched
				.source_evidence
				.iter()
				.any(|evidence| evidence.contains("affected range"))
		);
	}

	#[test]
	fn non_match_returns_no_finding() {
		let record = cve(
			json!("left-pad"),
			json!([{ "status": "affected", "version": "1.0.0", "lessThan": "1.2.0" }]),
		);

		let matches = match_kev_cve_record_to_reachable_npm(
			kev_context("CVE-2026-1001"),
			&record,
			&reachable("left-pad", "1.2.0"),
		);

		assert!(matches.is_empty());
	}

	#[test]
	fn ambiguous_package_name_returns_unknown_with_caveat() {
		let record = cve(
			Value::Null,
			json!([{ "status": "affected", "version": "1.0.0", "lessThan": "1.2.0" }]),
		);

		let matches = match_kev_cve_record_to_reachable_npm(
			kev_context("CVE-2026-1002"),
			&record,
			&reachable("left-pad", "1.1.0"),
		);

		assert_eq!(matches.len(), 1);
		assert_eq!(matches[0].status, KevNpmMatchStatus::Unknown);
		assert!(
			matches[0]
				.caveats
				.iter()
				.any(|caveat| caveat.contains("package name"))
		);
	}

	#[test]
	fn npm_kev_context_identifies_product_only_cna_metadata() {
		let record = json!({
			"containers": {
				"cna": {
					"affected": [{
						"vendor": "sebhildebrandt",
						"product": "systeminformation",
						"versions": [{ "status": "affected", "version": "< 5.3.1" }]
					}]
				}
			}
		});

		let matches = match_kev_cve_record_to_reachable_npm(
			npm_kev_context("CVE-2021-21315"),
			&record,
			&reachable("systeminformation", "5.3.0"),
		);

		assert_eq!(matches.len(), 1);
		assert_eq!(matches[0].status, KevNpmMatchStatus::Affected);
		assert_eq!(
			matches[0].package_name.as_deref(),
			Some("systeminformation")
		);
		assert_eq!(matches[0].affected_version.as_deref(), Some("5.3.0"));
	}

	#[test]
	fn product_only_cna_metadata_without_npm_kev_context_is_ignored() {
		let record = json!({
			"containers": {
				"cna": {
					"affected": [{
						"vendor": "sebhildebrandt",
						"product": "systeminformation",
						"versions": [{ "status": "affected", "version": "< 5.3.1" }]
					}]
				}
			}
		});

		let matches = match_kev_cve_record_to_reachable_npm(
			kev_context("CVE-2021-21315"),
			&record,
			&reachable("systeminformation", "5.3.0"),
		);

		assert_eq!(matches.len(), 1);
		assert_eq!(matches[0].status, KevNpmMatchStatus::Unknown);
		assert!(matches[0].package_name.is_none());
	}

	#[test]
	fn missing_version_ranges_returns_unknown_with_caveat() {
		let record = cve(json!("left-pad"), json!([]));

		let matches = match_kev_cve_record_to_reachable_npm(
			kev_context("CVE-2026-1003"),
			&record,
			&reachable("left-pad", "1.1.0"),
		);

		assert_eq!(matches.len(), 1);
		assert_eq!(matches[0].status, KevNpmMatchStatus::Unknown);
		assert!(
			matches[0]
				.caveats
				.iter()
				.any(|caveat| caveat.contains("version range"))
		);
	}

	#[test]
	fn kev_entry_without_usable_cve_enrichment_returns_unknown() {
		let matched = unknown_without_cve_enrichment(kev_context("CVE-2026-1004"));

		assert_eq!(matched.status, KevNpmMatchStatus::Unknown);
		assert_eq!(matched.confidence, KevNpmMatchConfidence::Unknown);
		assert!(
			matched
				.source_evidence
				.iter()
				.any(|evidence| evidence.contains("no usable active CVE List enrichment"))
		);
	}

	#[test]
	fn npm_v2_package_lock_versions_are_reachable_source_evidence() {
		let versions = reachable_npm_package_versions_from_source(include_str!(
			"../../testdata/npm/package-lock-v2.json"
		))
		.unwrap();

		assert_eq!(
			versions,
			vec![ReachableNpmPackageVersion {
				package_name: "left-pad".to_owned(),
				version: "1.1.0".to_owned(),
				source_evidence:
					"package-lock packages.node_modules/left-pad resolves left-pad@1.1.0".to_owned(),
			}]
		);
	}

	#[test]
	fn npm_v3_package_lock_derives_scoped_package_name_from_path() {
		let versions = reachable_npm_package_versions_from_source(include_str!(
			"../../testdata/npm/package-lock-v3.json"
		))
		.unwrap();

		assert_eq!(
			versions,
			vec![ReachableNpmPackageVersion {
				package_name: "@scope/name".to_owned(),
				version: "2.3.4".to_owned(),
				source_evidence:
					"package-lock packages.node_modules/@scope/name resolves @scope/name@2.3.4"
						.to_owned(),
			}]
		);
	}

	#[test]
	fn package_name_from_affected_entry_preserves_scoped_npm_names() {
		let entry = json!({
			"packageURL": "pkg:npm/@scope/name@1.2.3"
		});

		assert_eq!(
			package_name_from_affected_entry(&kev_context("CVE-2026-1006"), &entry).as_deref(),
			Some("@scope/name")
		);
	}

	#[test]
	fn package_name_from_affected_entry_decodes_encoded_scoped_npm_purl() {
		let entry = json!({
			"packageURL": "pkg:npm/%40scope/name@1.2.3"
		});

		assert_eq!(
			package_name_from_affected_entry(&kev_context("CVE-2026-1007"), &entry).as_deref(),
			Some("@scope/name")
		);
	}

	#[test]
	fn package_url_takes_precedence_over_package_name() {
		let entry = json!({
			"collectionURL": "https://www.npmjs.com/",
			"packageName": "wrong-name",
			"packageURL": "pkg:npm/right-name@1.2.3"
		});

		assert_eq!(
			package_name_from_affected_entry(&kev_context("CVE-2026-1008"), &entry).as_deref(),
			Some("right-name")
		);
	}

	#[test]
	fn collection_url_and_package_name_identify_npm_package() {
		let entry = json!({
			"collectionURL": "https://www.npmjs.com/",
			"packageName": "left-pad"
		});

		assert!(affected_entry_is_npm(&kev_context("CVE-2026-1009"), &entry));
		assert_eq!(
			package_name_from_affected_entry(&kev_context("CVE-2026-1009"), &entry).as_deref(),
			Some("left-pad")
		);
	}

	#[test]
	fn adp_affected_metadata_is_not_used_for_npm_matching() {
		let record = json!({
			"containers": {
				"adp": [{
					"affected": [{
						"collectionURL": "https://www.npmjs.com/",
						"packageName": "left-pad",
						"versions": [{ "status": "affected", "version": "1.0.0", "lessThan": "1.2.0" }]
					}]
				}]
			}
		});

		let matches = match_kev_cve_record_to_reachable_npm(
			kev_context("CVE-2026-1005"),
			&record,
			&reachable("left-pad", "1.1.0"),
		);

		assert_eq!(matches.len(), 1);
		assert_eq!(matches[0].status, KevNpmMatchStatus::Unknown);
		assert!(
			matches[0]
				.source_evidence
				.iter()
				.any(|evidence| evidence.contains("no NPM affected package metadata"))
		);
	}

	#[test]
	fn bulk_cve_lookup_uses_one_query_for_all_kev_entries() {
		let db = MockDatabase::new(DbBackend::Postgres)
			.append_query_results([vec![mock_kev_entry("CVE-2026-1006")], vec![]])
			.into_connection();

		let matches = run_async(kev_affected_npm_package_versions(&db, &[]))
			.expect("matching should succeed");

		assert_eq!(matches.len(), 1);
		assert_eq!(matches[0].status, KevNpmMatchStatus::Unknown);
		let transaction_log = db.into_transaction_log();
		assert_eq!(transaction_log.len(), 2);
		assert!(
			transaction_log[1].statements()[0]
				.sql
				.contains(r#""cve_list_records"."cve_id" IN"#)
		);
	}

	#[test]
	fn single_package_version_omits_unrelated_catalog_enrichment_gaps() {
		let db = MockDatabase::new(DbBackend::Postgres)
			.append_query_results([vec![mock_kev_entry("CVE-2026-1007")], vec![]])
			.into_connection();

		let matches = run_async(kev_affected_npm_package_version(
			&db,
			ReachableNpmPackageVersion {
				package_name: "left-pad".to_owned(),
				version: "1.1.0".to_owned(),
				source_evidence: "test query".to_owned(),
			},
		))
		.expect("matching should succeed");

		assert!(matches.is_empty());
	}

	fn mock_kev_entry(cve_id: &str) -> cisa_kev_entries::Model {
		let timestamp: sea_orm::prelude::DateTimeWithTimeZone =
			"2026-08-03T00:00:00Z".parse().expect("valid timestamp");
		cisa_kev_entries::Model {
			cve_id: cve_id.to_owned(),
			entry: json!({}),
			first_seen_at: timestamp,
			last_seen_at: timestamp,
			updated_at: timestamp,
			removed_at: None,
		}
	}

	fn run_async<T>(future: impl std::future::Future<Output = T>) -> T {
		tokio::runtime::Builder::new_current_thread()
			.enable_all()
			.build()
			.expect("test runtime should build")
			.block_on(future)
	}
}
