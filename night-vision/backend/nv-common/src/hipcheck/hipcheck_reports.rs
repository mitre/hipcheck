use super::{
	HipcheckCheckState, HipcheckEffect, HipcheckRecommendation, HipcheckReportContext,
	parse_hipcheck_report,
};
use std::error::Error as _;

const PASSING_REPORT: &str =
	include_str!("../../testdata/define-hipcheck/fixtures/native-315-passing.json");
const MIXED_REPORT: &str =
	include_str!("../../testdata/define-hipcheck/fixtures/native-315-mixed.json");
const INCOMPLETE_REPORT: &str =
	include_str!("../../testdata/define-hipcheck/fixtures/native-315-incomplete.json");
const MALFORMED_REPORT: &str =
	include_str!("../../testdata/define-hipcheck/fixtures/native-315-malformed.json");

fn context() -> HipcheckReportContext {
	HipcheckReportContext {
		target_purl: "pkg:npm/systeminformation@5.3.1".to_owned(),
		source_repository_url: "https://github.com/sebhildebrandt/systeminformation.git".to_owned(),
		policy_source: "/opt/night-vision/hipcheck/config/Hipcheck.kdl".to_owned(),
	}
}

#[test]
fn native_passing_report_normalizes_trusted_context_and_missing_evidence() {
	let report = parse_hipcheck_report(PASSING_REPORT, &context()).expect("native report parses");

	assert_eq!(report.schema_version, "1");
	assert_eq!(report.hipcheck.version, "3.15.0");
	assert_eq!(report.hipcheck.commit, "unknown");
	assert_eq!(report.target.kind, "npm");
	assert_eq!(
		report.target.purl.as_deref(),
		Some("pkg:npm/systeminformation@5.3.1")
	);
	assert_eq!(report.policy.id, "night-vision-upgrade-assessment");
	assert_eq!(report.policy.recommendation, HipcheckRecommendation::Pass);
	let check = &report.checks[0];
	assert_eq!(check.plugin.publisher, "mitre");
	assert_eq!(check.plugin.name, "binary");
	assert_eq!(check.plugin.version, "unknown");
	assert_eq!(check.state, HipcheckCheckState::Passed);
	assert_eq!(check.effect, HipcheckEffect::Context);
	assert_eq!(check.value["final_value"], "0");
	assert_eq!(check.concerns[0].kind, "missing-data");
}

#[test]
fn native_failing_and_errored_results_map_to_review_and_missing_check() {
	let report = parse_hipcheck_report(MIXED_REPORT, &context()).expect("native report parses");

	assert_eq!(
		report.policy.recommendation,
		HipcheckRecommendation::Investigate
	);
	assert_eq!(report.checks[0].state, HipcheckCheckState::Failed);
	assert_eq!(report.checks[0].effect, HipcheckEffect::Review);
	assert_eq!(report.checks[1].state, HipcheckCheckState::Errored);
	assert_eq!(report.checks[1].effect, HipcheckEffect::MissingCheck);
	assert_eq!(
		report.checks[1]
			.error
			.as_ref()
			.map(|error| error.kind.as_str()),
		Some("plugin")
	);
}

#[test]
fn native_report_rejects_missing_result_identity() {
	let error = parse_hipcheck_report(INCOMPLETE_REPORT, &context())
		.expect_err("missing plugin identity must fail");

	assert_eq!(error.to_string(), "invalid Hipcheck report JSON");
	assert!(
		error
			.source()
			.expect("JSON error is preserved")
			.to_string()
			.contains("missing field `name`")
	);
}

#[test]
fn malformed_native_report_is_rejected() {
	let error =
		parse_hipcheck_report(MALFORMED_REPORT, &context()).expect_err("JSON must be valid");

	assert_eq!(error.to_string(), "invalid Hipcheck report JSON");
}

#[test]
fn native_report_rejects_malformed_plugin_identity() {
	let input = PASSING_REPORT.replace("mitre/binary", "binary");
	let error =
		parse_hipcheck_report(&input, &context()).expect_err("plugin identity must be split");

	assert_eq!(
		error.detail(),
		Some("native check name must be publisher/name")
	);
}
