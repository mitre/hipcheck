//! Contract tests for the Hipcheck JSON consumer boundary.
//!
//! The fixtures implement the report shape documented in RFD 0002 and
//! `docs/backend/hipcheck-integration.md`.

use super::{HipcheckCheckState, HipcheckRecommendation, parse_hipcheck_report};

const VALID_MVP_REPORT: &str =
    include_str!("../../testdata/define-hipcheck/fixtures/valid_mvp.json");
const MISSING_OPTIONAL_FIELDS_REPORT: &str =
    include_str!("../../testdata/define-hipcheck/fixtures/missing_optional_fields.json");
const MISSING_REQUIRED_FIELDS_REPORT: &str =
    include_str!("../../testdata/define-hipcheck/fixtures/missing_required_fields.json");

#[test]
fn valid_mvp_report_deserializes() {
    let report = parse_hipcheck_report(VALID_MVP_REPORT).expect("valid MVP report");

    assert_eq!(report.schema_version, "1");
    assert_eq!(report.hipcheck.version, "4.0.0-pre");
    assert_eq!(report.target.kind, "npm");
    assert_eq!(
        report.target.purl.as_deref(),
        Some("pkg:npm/%40scope/name@1.2.7")
    );
    assert_eq!(report.policy.recommendation, HipcheckRecommendation::Pass);
    assert_eq!(report.checks.len(), 1);
    assert_eq!(report.checks[0].state, HipcheckCheckState::Passed);
}

#[test]
fn missing_optional_fields_still_parse() {
    let report = parse_hipcheck_report(MISSING_OPTIONAL_FIELDS_REPORT)
        .expect("report without optional evidence");

    let check = &report.checks[0];
    assert!(report.target.purl.is_none());
    assert!(report.policy.version.is_none());
    assert!(check.severity.is_none());
    assert!(check.concerns.is_empty());
    assert!(check.started_at.is_none());
    assert!(check.ended_at.is_none());
    assert!(check.error.is_none());
}

#[test]
fn missing_required_fields_fixture_is_rejected() {
    let error = parse_hipcheck_report(MISSING_REQUIRED_FIELDS_REPORT)
        .expect_err("report is missing required target identity");

    assert!(
        error
            .to_string()
            .contains("missing field `source_repository_url`")
    );
}

#[test]
fn missing_required_target_identity_is_rejected() {
    let input = VALID_MVP_REPORT.replacen(
        ",\n    \"source_repository_url\": \"https://github.com/example/name\"",
        "",
        1,
    );

    let error = parse_hipcheck_report(&input).expect_err("source repository URL is required");

    assert!(
        error
            .to_string()
            .contains("missing field `source_repository_url`")
    );
}

#[test]
fn empty_or_malformed_source_repository_url_is_rejected() {
    for invalid_url in ["", "not a URL", "file:///tmp/repository"] {
        let input = VALID_MVP_REPORT.replace("https://github.com/example/name", invalid_url);

        let error = parse_hipcheck_report(&input)
            .expect_err("source repository URL must be an absolute HTTP(S) URL");

        assert_eq!(
            error.detail(),
            Some("target.source_repository_url must be an absolute HTTP(S) URL")
        );
    }
}

#[test]
fn missing_required_check_state_is_rejected() {
    let input = VALID_MVP_REPORT.replacen("\n      \"state\": \"passed\",", "", 1);

    let error = parse_hipcheck_report(&input).expect_err("check state is required");

    assert!(error.to_string().contains("missing field `state`"));
}

#[test]
fn missing_required_structured_value_is_rejected() {
    let input = VALID_MVP_REPORT.replacen(
        "\n      \"value\": {\n        \"installScriptsChanged\": false,\n        \"packageSizeDelta\": 0\n      },",
        "",
        1,
    );

    let error = parse_hipcheck_report(&input).expect_err("check value is required");

    assert!(error.to_string().contains("missing field `value`"));
}

#[test]
fn malformed_json_is_rejected_with_a_parse_error() {
    let error = parse_hipcheck_report("{").expect_err("JSON must be complete");

    assert!(
        error
            .to_string()
            .starts_with("invalid Hipcheck report JSON:")
    );
}

#[test]
fn npm_target_without_a_versioned_purl_is_rejected() {
    let input = VALID_MVP_REPORT.replace("pkg:npm/%40scope/name@1.2.7", "pkg:npm/%40scope/name");

    let error = parse_hipcheck_report(&input).expect_err("npm PURL requires a version");

    assert_eq!(
        error.detail(),
        Some("target.purl must be a versioned npm package URL for npm targets")
    );
}

#[test]
fn npm_target_with_malformed_purl_is_rejected() {
    for malformed_purl in [
        "pkg:npm/name@1.2.7@bogus",
        "pkg:npm/%40scope/name@",
        "pkg:npm/%40scope/@1.2.7",
        "pkg:npm/name%2@1.2.7",
    ] {
        let input = VALID_MVP_REPORT.replace("pkg:npm/%40scope/name@1.2.7", malformed_purl);

        let error = parse_hipcheck_report(&input)
            .expect_err("npm target PURLs must contain one package name and version");

        assert_eq!(
            error.detail(),
            Some("target.purl must be a versioned npm package URL for npm targets")
        );
    }
}
