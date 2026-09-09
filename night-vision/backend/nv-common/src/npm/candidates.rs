//! Discovers and validates NPM upgrade candidates.
//!
//! Given a packument (see [`crate::npm::packument`]) and a known-vulnerable
//! package version, [`discover_upgrade_candidates`] finds newer published
//! versions and annotates each with whether it's usable as an upgrade target
//! and what SemVer API compatibility guarantee applies.
//! [`validate_explicit_candidate`] covers the
//! case where a caller already knows which version they want to assess,
//! bypassing discovery while still checking that the version is real.
//!
//! Both functions operate purely on already-parsed packument data — no
//! registry fetch, no database, no ranges. "Strictly newer" is a plain
//! [`Version`] comparison; it deliberately does not use [`crate::npm_semver`],
//! which answers a different question (does a version satisfy a range like
//! `^1.2.3`) using a different version type.

use std::fmt::{Display, Formatter, Result as FmtResult};

use jiff::Timestamp;
use semver::Version;

use super::packument::{NpmPackageName, NpmPackument};
use super::types::NpmPackageNameError;

/// An upgrade candidate discovered or validated against packument data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpgradeCandidate {
    pub name: NpmPackageName,
    pub version: Version,
    /// `pkg:npm/name@version`, or `pkg:npm/%40scope/name@version` for scoped packages.
    pub purl: String,
    /// The version's publish time, when the packument carries that data.
    pub published_at: Option<Timestamp>,
    /// The SemVer distance from the vulnerable version to this candidate.
    pub upgrade_distance: UpgradeDistance,
    /// The API compatibility guarantee SemVer provides for this upgrade.
    pub api_compatibility: ApiCompatibility,
    pub status: CandidateStatus,
}

/// The SemVer distance between an affected version and a newer candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpgradeDistance {
    Patch,
    Minor,
    Major,
}

/// The compatibility guarantee SemVer provides between two package versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiCompatibility {
    /// A stable package's major version is unchanged, so SemVer promises
    /// compatibility with its previous public API.
    Compatible,
    /// A stable package's major version changed. Its API may be incompatible;
    /// SemVer requires that version bump when it makes incompatible changes.
    Incompatible,
    /// One of the versions is `0.y.z` or a prerelease, for which SemVer makes
    /// no public API stability guarantee.
    NoGuarantee,
}

/// Whether a candidate is usable, or the reasons it isn't.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CandidateStatus {
    Included,
    /// Never empty: a version can be excluded for more than one reason at once
    /// (for example, a deprecated prerelease).
    Excluded(Vec<CandidateExclusionReason>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CandidateExclusionReason {
    Prerelease,
    /// Carries the deprecation message directly so it's usable in a
    /// user-facing explanation without cross-referencing another field.
    Deprecated(Box<str>),
}

/// Finds newer published versions of `packument`'s package.
///
/// Every version strictly newer than `vulnerable_version` is returned in
/// ascending version order, annotated with its [`ApiCompatibility`] and
/// [`CandidateStatus`]. An empty result (no newer version) is `Ok(vec![])`,
/// not an error.
///
/// `vulnerable_version` does not need to be present in `packument.versions`:
/// npm allows unpublishing an individual version within its unpublish
/// window, so a vulnerability record can legitimately name a version the
/// registry no longer serves. It's used only as a comparison baseline.
pub fn discover_upgrade_candidates(
    packument: &NpmPackument,
    vulnerable_version: &str,
) -> Result<Vec<UpgradeCandidate>, CandidateDiscoveryError> {
    let vulnerable_version = parse_version(vulnerable_version)
        .map_err(|(value, source)| CandidateDiscoveryError::MalformedVersion { value, source })?;

    let mut candidates: Vec<UpgradeCandidate> = packument
        .versions
        .values()
        .filter(|version| version.version > vulnerable_version)
        .map(|version| build_candidate(packument, &vulnerable_version, version))
        .collect();

    candidates.sort_by(|a, b| a.version.cmp(&b.version));

    Ok(candidates)
}

/// Validates a caller-supplied candidate version against packument data,
/// bypassing discovery.
///
/// Checks that `package_name` matches `packument`'s own name (package
/// identity) and that `candidate_version` is actually published (version
/// availability), returning an error otherwise. Unlike discovery, a
/// deprecated or prerelease candidate does not fail validation here — the
/// caller deliberately chose this version, so it comes back as
/// `Ok(UpgradeCandidate { status: CandidateStatus::Excluded(..), .. })` for the
/// assessment layer to decide whether to proceed with a caveat.
pub fn validate_explicit_candidate(
    packument: &NpmPackument,
    package_name: &str,
    vulnerable_version: &str,
    candidate_version: &str,
) -> Result<UpgradeCandidate, CandidateValidationError> {
    let requested_name = NpmPackageName::parse(package_name.to_owned())
        .map_err(CandidateValidationError::MalformedPackageName)?;

    if requested_name != packument.name {
        return Err(CandidateValidationError::PackageIdentityMismatch {
            requested: requested_name,
            packument: packument.name.clone(),
        });
    }

    let candidate_version = parse_version(candidate_version)
        .map_err(|(value, source)| CandidateValidationError::MalformedVersion { value, source })?;
    let vulnerable_version = parse_version(vulnerable_version)
        .map_err(|(value, source)| CandidateValidationError::MalformedVersion { value, source })?;

    if candidate_version <= vulnerable_version {
        return Err(CandidateValidationError::NotNewer {
            current: vulnerable_version,
            candidate: candidate_version,
        });
    }

    let version = packument.versions.get(&candidate_version).ok_or(
        CandidateValidationError::VersionNotAvailable(candidate_version),
    )?;

    Ok(build_candidate(packument, &vulnerable_version, version))
}

fn build_candidate(
    packument: &NpmPackument,
    vulnerable_version: &Version,
    version: &super::packument::NpmVersion,
) -> UpgradeCandidate {
    let mut reasons = Vec::new();

    if !version.version.pre.is_empty() {
        reasons.push(CandidateExclusionReason::Prerelease);
    }

    if let Some(message) = &version.deprecated {
        reasons.push(CandidateExclusionReason::Deprecated(
            message.as_str().into(),
        ));
    }

    let status = if reasons.is_empty() {
        CandidateStatus::Included
    } else {
        CandidateStatus::Excluded(reasons)
    };

    UpgradeCandidate {
        name: packument.name.clone(),
        version: version.version.clone(),
        purl: build_purl(&packument.name, &version.version),
        published_at: published_at(packument, &version.version),
        upgrade_distance: upgrade_distance(vulnerable_version, &version.version),
        api_compatibility: api_compatibility(vulnerable_version, &version.version),
        status,
    }
}

/// Classifies how far a newer candidate moves from an affected version.
pub fn upgrade_distance(
    vulnerable_version: &Version,
    candidate_version: &Version,
) -> UpgradeDistance {
    if vulnerable_version.major != candidate_version.major {
        UpgradeDistance::Major
    } else if vulnerable_version.minor != candidate_version.minor {
        UpgradeDistance::Minor
    } else {
        UpgradeDistance::Patch
    }
}

/// Classifies the SemVer API compatibility guarantee for an upgrade.
pub fn api_compatibility(
    vulnerable_version: &Version,
    candidate_version: &Version,
) -> ApiCompatibility {
    if vulnerable_version.major == 0
        || candidate_version.major == 0
        || !vulnerable_version.pre.is_empty()
        || !candidate_version.pre.is_empty()
    {
        ApiCompatibility::NoGuarantee
    } else if vulnerable_version.major == candidate_version.major {
        ApiCompatibility::Compatible
    } else {
        ApiCompatibility::Incompatible
    }
}

fn published_at(packument: &NpmPackument, version: &Version) -> Option<Timestamp> {
    let times = packument.time.as_ref()?;
    let version = version.to_string();
    times
        .versions
        .iter()
        .find(|(key, _)| key.as_str() == version)
        .map(|(_, timestamp)| *timestamp)
}

/// Builds a `pkg:npm/...` PURL, percent-encoding the `@` in a scoped
/// package's namespace segment per the purl spec's npm-type convention
/// (e.g. `pkg:npm/%40angular/animation@12.3.1`).
fn build_purl(name: &NpmPackageName, version: &Version) -> String {
    match name
        .as_str()
        .strip_prefix('@')
        .and_then(|rest| rest.split_once('/'))
    {
        Some((scope, package)) => format!("pkg:npm/%40{scope}/{package}@{version}"),
        None => format!("pkg:npm/{}@{version}", name.as_str()),
    }
}

fn parse_version(value: &str) -> Result<Version, (Box<str>, semver::Error)> {
    Version::parse(value).map_err(|error| (value.into(), error))
}

#[derive(Debug)]
pub enum CandidateDiscoveryError {
    MalformedVersion {
        value: Box<str>,
        source: semver::Error,
    },
}

impl Display for CandidateDiscoveryError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::MalformedVersion { value, .. } => {
                write!(f, "malformed vulnerable version {value:?}")
            }
        }
    }
}

impl std::error::Error for CandidateDiscoveryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::MalformedVersion { source, .. } => Some(source),
        }
    }
}

#[derive(Debug)]
pub enum CandidateValidationError {
    MalformedPackageName(NpmPackageNameError),
    PackageIdentityMismatch {
        requested: NpmPackageName,
        packument: NpmPackageName,
    },
    MalformedVersion {
        value: Box<str>,
        source: semver::Error,
    },
    VersionNotAvailable(Version),
    NotNewer {
        current: Version,
        candidate: Version,
    },
}

impl Display for CandidateValidationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::MalformedPackageName(_) => write!(f, "malformed package name"),
            Self::PackageIdentityMismatch {
                requested,
                packument,
            } => write!(
                f,
                "requested package {} does not match packument package {}",
                requested.as_str(),
                packument.as_str()
            ),
            Self::MalformedVersion { value, .. } => {
                write!(f, "malformed candidate version {value:?}")
            }
            Self::VersionNotAvailable(version) => {
                write!(f, "candidate version {version} is not published")
            }
            Self::NotNewer { current, candidate } => {
                write!(
                    f,
                    "candidate version {candidate} is not newer than {current}"
                )
            }
        }
    }
}

impl std::error::Error for CandidateValidationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::MalformedPackageName(source) => Some(source),
            Self::MalformedVersion { source, .. } => Some(source),
            Self::PackageIdentityMismatch { .. }
            | Self::VersionNotAvailable(_)
            | Self::NotNewer { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::npm::packument::parse_packument;
    use serde_json::{Value, json};
    use std::fs::File;

    fn version_entry(version: &str, extra: Value) -> (String, Value) {
        let mut entry = json!({
            "name": "example",
            "version": version,
            "dist": {
                "tarball": "https://registry.npmjs.org/example/-/example.tgz",
                "shasum": "0123456789abcdef0123456789abcdef01234567"
            }
        });
        if let Value::Object(extra) = extra {
            entry.as_object_mut().unwrap().extend(extra);
        }
        (version.to_owned(), entry)
    }

    fn packument(name: &str, versions: Vec<(String, Value)>) -> NpmPackument {
        let dist_tags = json!({ "latest": versions.last().unwrap().0.clone() });
        let versions: serde_json::Map<String, Value> = versions.into_iter().collect();

        let value = json!({
            "name": name,
            "dist-tags": dist_tags,
            "versions": versions,
        });

        parse_packument(serde_json::to_vec(&value).unwrap().as_slice())
            .expect("test packument should parse")
    }

    fn packument_with_times(
        name: &str,
        versions: Vec<(String, Value)>,
        times: Vec<(&str, &str)>,
    ) -> NpmPackument {
        let dist_tags = json!({ "latest": versions.last().unwrap().0.clone() });
        let versions_map: serde_json::Map<String, Value> = versions.into_iter().collect();

        let mut time = serde_json::Map::new();
        time.insert("created".to_owned(), json!("2024-01-01T00:00:00.000Z"));
        time.insert("modified".to_owned(), json!("2024-01-01T00:00:00.000Z"));
        for (version, timestamp) in times {
            time.insert(version.to_owned(), json!(timestamp));
        }

        let value = json!({
            "name": name,
            "dist-tags": dist_tags,
            "versions": versions_map,
            "time": time,
        });

        parse_packument(serde_json::to_vec(&value).unwrap().as_slice())
            .expect("test packument should parse")
    }

    #[test]
    fn discovery_returns_empty_when_no_newer_patch_exists() {
        let packument = packument("example", vec![version_entry("1.2.3", json!({}))]);

        let candidates = discover_upgrade_candidates(&packument, "1.2.3").unwrap();

        assert!(candidates.is_empty());
    }

    #[test]
    fn discovery_includes_a_newer_patch_version() {
        let packument = packument(
            "example",
            vec![
                version_entry("1.2.3", json!({})),
                version_entry("1.2.4", json!({})),
            ],
        );

        let candidates = discover_upgrade_candidates(&packument, "1.2.3").unwrap();

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].version, Version::parse("1.2.4").unwrap());
        assert_eq!(candidates[0].purl, "pkg:npm/example@1.2.4");
        assert_eq!(
            candidates[0].api_compatibility,
            ApiCompatibility::Compatible
        );
        assert_eq!(candidates[0].status, CandidateStatus::Included);
    }

    #[test]
    fn discovery_includes_and_classifies_minor_and_major_upgrades() {
        let packument = packument(
            "example",
            vec![
                version_entry("1.2.3", json!({})),
                version_entry("1.3.0", json!({})),
                version_entry("2.0.0", json!({})),
            ],
        );

        let candidates = discover_upgrade_candidates(&packument, "1.2.3").unwrap();

        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].version, Version::parse("1.3.0").unwrap());
        assert_eq!(candidates[0].upgrade_distance, UpgradeDistance::Minor);
        assert_eq!(
            candidates[0].api_compatibility,
            ApiCompatibility::Compatible
        );
        assert_eq!(candidates[1].version, Version::parse("2.0.0").unwrap());
        assert_eq!(candidates[1].upgrade_distance, UpgradeDistance::Major);
        assert_eq!(
            candidates[1].api_compatibility,
            ApiCompatibility::Incompatible
        );
    }

    #[test]
    fn discovery_reports_no_compatibility_guarantee_for_zero_major_versions() {
        let packument = packument(
            "example",
            vec![
                version_entry("0.2.3", json!({})),
                version_entry("0.2.4", json!({})),
                version_entry("0.3.0", json!({})),
                version_entry("1.0.0", json!({})),
            ],
        );

        let candidates = discover_upgrade_candidates(&packument, "0.2.3").unwrap();

        assert!(
            candidates
                .iter()
                .all(|candidate| candidate.api_compatibility == ApiCompatibility::NoGuarantee)
        );
    }

    #[test]
    fn discovery_excludes_prerelease_only() {
        let packument = packument(
            "example",
            vec![
                version_entry("1.2.3", json!({})),
                version_entry("1.2.4-beta.1", json!({})),
            ],
        );

        let candidates = discover_upgrade_candidates(&packument, "1.2.3").unwrap();

        assert_eq!(candidates.len(), 1);
        assert_eq!(
            candidates[0].status,
            CandidateStatus::Excluded(vec![CandidateExclusionReason::Prerelease])
        );
        assert_eq!(
            candidates[0].api_compatibility,
            ApiCompatibility::NoGuarantee
        );
    }

    #[test]
    fn discovery_excludes_deprecated_only() {
        let packument = packument(
            "example",
            vec![
                version_entry("1.2.3", json!({})),
                version_entry("1.2.4", json!({ "deprecated": "use 1.2.5 instead" })),
            ],
        );

        let candidates = discover_upgrade_candidates(&packument, "1.2.3").unwrap();

        assert_eq!(candidates.len(), 1);
        assert_eq!(
            candidates[0].status,
            CandidateStatus::Excluded(vec![CandidateExclusionReason::Deprecated(
                "use 1.2.5 instead".into()
            )])
        );
    }

    #[test]
    fn discovery_excludes_legacy_boolean_deprecation() {
        let packument = packument(
            "example",
            vec![
                version_entry("1.2.3", json!({})),
                version_entry("1.2.4", json!({ "deprecated": true })),
            ],
        );

        let candidates = discover_upgrade_candidates(&packument, "1.2.3").unwrap();

        assert_eq!(
            candidates[0].status,
            CandidateStatus::Excluded(vec![CandidateExclusionReason::Deprecated(
                "package is deprecated".into()
            )])
        );
    }

    #[test]
    fn discovery_excludes_a_version_that_is_both_prerelease_and_deprecated() {
        let packument = packument(
            "example",
            vec![
                version_entry("1.2.3", json!({})),
                version_entry("1.2.4-beta.1", json!({ "deprecated": "broken prerelease" })),
            ],
        );

        let candidates = discover_upgrade_candidates(&packument, "1.2.3").unwrap();

        assert_eq!(candidates.len(), 1);
        assert_eq!(
            candidates[0].status,
            CandidateStatus::Excluded(vec![
                CandidateExclusionReason::Prerelease,
                CandidateExclusionReason::Deprecated("broken prerelease".into()),
            ])
        );
    }

    #[test]
    fn discovery_orders_candidates_ascending() {
        let packument = packument(
            "example",
            vec![
                version_entry("1.2.3", json!({})),
                version_entry("1.2.6", json!({})),
                version_entry("1.2.4", json!({})),
                version_entry("1.2.5", json!({})),
            ],
        );

        let candidates = discover_upgrade_candidates(&packument, "1.2.3").unwrap();

        let versions: Vec<String> = candidates
            .iter()
            .map(|candidate| candidate.version.to_string())
            .collect();
        assert_eq!(versions, ["1.2.4", "1.2.5", "1.2.6"]);
    }

    #[test]
    fn discovery_rejects_malformed_vulnerable_version() {
        let packument = packument("example", vec![version_entry("1.2.3", json!({}))]);

        let error = discover_upgrade_candidates(&packument, "not-a-version").unwrap_err();

        assert!(matches!(
            error,
            CandidateDiscoveryError::MalformedVersion { .. }
        ));
    }

    #[test]
    fn discovery_leaves_published_at_unset_without_time_data() {
        let packument = packument(
            "example",
            vec![
                version_entry("1.2.3", json!({})),
                version_entry("1.2.4", json!({})),
            ],
        );

        let candidates = discover_upgrade_candidates(&packument, "1.2.3").unwrap();

        assert_eq!(candidates[0].published_at, None);
    }

    #[test]
    fn discovery_reports_published_at_when_time_data_is_present() {
        let packument = packument_with_times(
            "example",
            vec![
                version_entry("1.2.3", json!({})),
                version_entry("1.2.4", json!({})),
            ],
            vec![
                ("1.2.3", "2024-01-01T00:00:00.000Z"),
                ("1.2.4", "2024-02-01T00:00:00.000Z"),
            ],
        );

        let candidates = discover_upgrade_candidates(&packument, "1.2.3").unwrap();

        assert_eq!(
            candidates[0].published_at,
            Some("2024-02-01T00:00:00Z".parse().unwrap())
        );
    }

    #[test]
    fn discovery_does_not_require_the_vulnerable_version_to_be_published() {
        let packument = packument("example", vec![version_entry("1.2.4", json!({}))]);

        let candidates = discover_upgrade_candidates(&packument, "1.2.3").unwrap();

        assert_eq!(candidates.len(), 1);
    }

    #[test]
    fn explicit_validation_accepts_a_published_version() {
        let packument = packument(
            "example",
            vec![
                version_entry("1.2.3", json!({})),
                version_entry("1.2.4", json!({})),
            ],
        );

        let candidate =
            validate_explicit_candidate(&packument, "example", "1.2.3", "1.2.4").unwrap();

        assert_eq!(candidate.version, Version::parse("1.2.4").unwrap());
        assert_eq!(candidate.api_compatibility, ApiCompatibility::Compatible);
        assert_eq!(candidate.status, CandidateStatus::Included);
    }

    #[test]
    fn explicit_validation_rejects_same_or_older_versions() {
        let packument = packument(
            "example",
            vec![
                version_entry("1.2.2", json!({})),
                version_entry("1.2.3", json!({})),
            ],
        );

        for version in ["1.2.2", "1.2.3"] {
            assert!(matches!(
                validate_explicit_candidate(&packument, "example", "1.2.3", version),
                Err(CandidateValidationError::NotNewer { .. })
            ));
        }
    }

    #[test]
    fn explicit_validation_rejects_an_unpublished_version() {
        let packument = packument("example", vec![version_entry("1.2.3", json!({}))]);

        let error =
            validate_explicit_candidate(&packument, "example", "1.2.3", "9.9.9").unwrap_err();

        assert!(matches!(
            error,
            CandidateValidationError::VersionNotAvailable(_)
        ));
    }

    #[test]
    fn explicit_validation_rejects_a_package_identity_mismatch() {
        let packument = packument("example", vec![version_entry("1.2.3", json!({}))]);

        let error = validate_explicit_candidate(&packument, "some-other-package", "1.2.3", "1.2.3")
            .unwrap_err();

        assert!(matches!(
            error,
            CandidateValidationError::PackageIdentityMismatch { .. }
        ));
    }

    #[test]
    fn explicit_validation_rejects_a_malformed_package_name() {
        let packument = packument("example", vec![version_entry("1.2.3", json!({}))]);

        let error = validate_explicit_candidate(&packument, "", "1.2.3", "1.2.3").unwrap_err();

        assert!(matches!(
            error,
            CandidateValidationError::MalformedPackageName(_)
        ));
    }

    #[test]
    fn explicit_validation_rejects_a_malformed_version() {
        let packument = packument("example", vec![version_entry("1.2.3", json!({}))]);

        let error = validate_explicit_candidate(&packument, "example", "1.2.3", "not-a-version")
            .unwrap_err();

        assert!(matches!(
            error,
            CandidateValidationError::MalformedVersion { .. }
        ));
    }

    #[test]
    fn explicit_validation_allows_a_deprecated_candidate_without_erroring() {
        let packument = packument(
            "example",
            vec![version_entry(
                "1.2.3",
                json!({ "deprecated": "known issue" }),
            )],
        );

        let candidate =
            validate_explicit_candidate(&packument, "example", "1.2.2", "1.2.3").unwrap();

        assert_eq!(
            candidate.status,
            CandidateStatus::Excluded(vec![CandidateExclusionReason::Deprecated(
                "known issue".into()
            )])
        );
    }

    #[test]
    fn scoped_package_names_produce_percent_encoded_purls() {
        let value = json!({
            "name": "@scope/example",
            "dist-tags": { "latest": "1.0.0" },
            "versions": {
                "1.0.0": {
                    "name": "@scope/example",
                    "version": "1.0.0",
                    "dist": {
                        "tarball": "https://registry.npmjs.org/@scope/example/-/example-1.0.0.tgz",
                        "shasum": "0123456789abcdef0123456789abcdef01234567"
                    }
                },
                "1.0.1": {
                    "name": "@scope/example",
                    "version": "1.0.1",
                    "dist": {
                        "tarball": "https://registry.npmjs.org/@scope/example/-/example-1.0.1.tgz",
                        "shasum": "0123456789abcdef0123456789abcdef01234567"
                    }
                }
            }
        });
        let packument = parse_packument(serde_json::to_vec(&value).unwrap().as_slice()).unwrap();

        let candidates = discover_upgrade_candidates(&packument, "1.0.0").unwrap();

        assert_eq!(candidates[0].purl, "pkg:npm/%40scope/example@1.0.1");
    }

    #[test]
    fn discovery_runs_without_error_against_a_real_world_corpus_fixture() {
        let path = format!(
            "{}/testdata/npm/packument/real/express.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let file = File::open(&path).unwrap();
        let packument = parse_packument(file).unwrap();

        let oldest_version = packument
            .versions
            .keys()
            .min()
            .expect("express fixture has versions")
            .to_string();

        discover_upgrade_candidates(&packument, &oldest_version)
            .expect("discovery should not error against real-world data");
    }
}
