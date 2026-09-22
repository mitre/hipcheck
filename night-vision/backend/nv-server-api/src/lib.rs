// We'd prefer to use `jiff` over `chrono`, but `dropshot` depends on
// an old version of `schemars` that doesn't support `jiff`. When we
// can use a newer version of `schemars`, we should switch to using
// `jiff`.
use chrono::{DateTime, Utc};
use dropshot::{HttpError, HttpResponseAccepted, HttpResponseOk, Path, RequestContext, TypedBody};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// Maximum encoded JSON request size for a package-source submission.
///
/// This accommodates a maximally escaped 1 MiB `package.json` document plus
/// the JSON request envelope.
pub const MAX_PACKAGE_SOURCE_REQUEST_BODY_BYTES: usize = 3 * 1024 * 1024;

/// Maximum encoded JSON request size for an upgrade-assessment submission.
///
/// This accommodates a package source payload equivalent to a package-source
/// upload plus the surrounding assessment-specific request envelope.
pub const MAX_UPGRADE_ASSESSMENT_REQUEST_BODY_BYTES: usize = 3 * 1024 * 1024;

#[dropshot::api_description]
pub trait NvServerApi {
    type Context: Send + Sync + 'static;

    #[endpoint {
        method = GET,
        path = "/health",
    }]
    async fn health(
        ctx: RequestContext<Self::Context>,
    ) -> Result<HttpResponseOk<Health>, HttpError>;

    #[endpoint {
        method = GET,
        path = "/health/diagnostics",
    }]
    async fn health_diagnostics(
        ctx: RequestContext<Self::Context>,
    ) -> Result<HttpResponseOk<HealthDiagnostics>, HttpError>;

    #[endpoint {
        method = POST,
        path = "/package-sources",
        content_type = "application/json",
        request_body_max_bytes = MAX_PACKAGE_SOURCE_REQUEST_BODY_BYTES,
    }]
    async fn post_package_source(
        ctx: RequestContext<Self::Context>,
        body_param: TypedBody<PostPackageSourceBody>,
    ) -> Result<HttpResponseAccepted<PostPackageSourceResponse>, HttpError>;

    #[endpoint {
        method = GET,
        path = "/package-sources/{id}",
    }]
    async fn get_package_source(
        ctx: RequestContext<Self::Context>,
        path_params: Path<PackageSourcePathParams>,
    ) -> Result<HttpResponseOk<PackageSourceStatus>, HttpError>;

    #[endpoint { method = POST, path = "/package-sources/{id}/cancel" }]
    async fn cancel_package_source(
        ctx: RequestContext<Self::Context>,
        path_params: Path<PackageSourcePathParams>,
    ) -> Result<HttpResponseAccepted<PackageSourceOperationResponse>, HttpError>;

    #[endpoint { method = DELETE, path = "/package-sources/{id}" }]
    async fn delete_package_source(
        ctx: RequestContext<Self::Context>,
        path_params: Path<PackageSourcePathParams>,
    ) -> Result<HttpResponseAccepted<PackageSourceOperationResponse>, HttpError>;

    #[endpoint { method = GET, path = "/package-sources/{id}/exposures", }]
    async fn get_package_source_exposures(
        ctx: RequestContext<Self::Context>,
        path_params: Path<PackageSourcePathParams>,
    ) -> Result<HttpResponseOk<PackageSourceExposures>, HttpError>;

    #[endpoint { method = POST, path = "/assessments", content_type = "application/json" }]
    async fn post_assessment(
        ctx: RequestContext<Self::Context>,
        body_param: TypedBody<PostAssessmentBody>,
    ) -> Result<HttpResponseAccepted<PostAssessmentResponse>, HttpError>;

    #[endpoint { method = GET, path = "/assessments/{id}" }]
    async fn get_assessment(
        ctx: RequestContext<Self::Context>,
        path_params: Path<AssessmentPathParams>,
    ) -> Result<HttpResponseOk<AssessmentStatus>, HttpError>;

    #[endpoint { method = GET, path = "/assessments/{id}/evidence" }]
    async fn get_assessment_evidence(
        ctx: RequestContext<Self::Context>,
        path_params: Path<AssessmentPathParams>,
        query: dropshot::Query<AssessmentEvidenceQuery>,
    ) -> Result<HttpResponseOk<AssessmentEvidence>, HttpError>;

    #[endpoint {
        method = POST,
        path = "/upgrade-assessments",
        content_type = "application/json",
        request_body_max_bytes = MAX_UPGRADE_ASSESSMENT_REQUEST_BODY_BYTES,
    }]
    async fn post_upgrade_assessment(
        ctx: RequestContext<Self::Context>,
        body_param: TypedBody<PostUpgradeAssessmentBody>,
    ) -> Result<HttpResponseAccepted<PostUpgradeAssessmentResponse>, HttpError>;

    #[endpoint {
        method = GET,
        path = "/upgrade-assessments/{id}",
    }]
    async fn get_upgrade_assessment(
        ctx: RequestContext<Self::Context>,
        path_params: Path<UpgradeAssessmentPathParams>,
    ) -> Result<HttpResponseOk<UpgradeAssessmentStatus>, HttpError>;

    #[endpoint {
        method = GET,
        path = "/upgrade-assessments/{id}/result",
    }]
    async fn get_upgrade_assessment_result(
        ctx: RequestContext<Self::Context>,
        path_params: Path<UpgradeAssessmentPathParams>,
    ) -> Result<HttpResponseOk<UpgradeAssessmentResult>, HttpError>;
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PostAssessmentBody {
    #[serde(rename = "affectedPurl")]
    pub affected_purl: String,
    #[serde(rename = "targetPurl")]
    pub target_purl: String,
}
#[derive(Serialize, JsonSchema)]
pub struct PostAssessmentResponse {
    pub id: Uuid,
}
#[derive(Deserialize, JsonSchema)]
pub struct AssessmentPathParams {
    pub id: Uuid,
}
#[derive(Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct AssessmentEvidenceQuery {
    pub include_raw_hipcheck: Option<bool>,
}
#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AssessmentStatus {
    pub id: Uuid,
    pub state: String,
    pub affected_purl: Option<String>,
    pub target: Option<String>,
    pub source_repository_url: Option<String>,
    pub recommendation: Option<String>,
    pub finding_count: usize,
    pub exit_status: Option<i32>,
    pub error_kind: Option<String>,
    pub error_message: Option<String>,
    pub retryable: Option<bool>,
}
#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AssessmentEvidence {
    pub id: Uuid,
    pub affected_purl: Option<String>,
    pub diagnostics: AssessmentDiagnostics,
    pub checks: Vec<AssessmentCheck>,
    pub findings: Vec<AssessmentFinding>,
    pub raw_hipcheck: Option<String>,
}
#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AssessmentDiagnostics {
    pub source_repository_url: Option<String>,
    pub stdout: Option<String>,
    pub stdout_truncated: bool,
    pub stderr: Option<String>,
    pub stderr_truncated: bool,
    pub exit_status: Option<i32>,
    pub error_kind: Option<String>,
    pub error_message: Option<String>,
    pub retryable: Option<bool>,
}
#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AssessmentCheck {
    pub state: String,
    pub effect: String,
    pub summary: String,
}
#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AssessmentFinding {
    pub kind: String,
    pub effect: String,
    pub severity: Option<String>,
    pub summary: String,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Health {
    pub status: String,
}

/// Health information for operators, not untrusted API callers.
#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HealthDiagnostics {
    pub status: String,
    pub cve_ingest: CveIngestHealth,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CveIngestHealth {
    pub records_available: bool,
    pub latest_successful_commit: Option<String>,
    pub latest_run: Option<CveListSyncRunHealth>,
}

#[derive(Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CveListSyncRunHealth {
    pub generation: i64,
    pub status: String,
    pub checked_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub records_seen: i32,
    pub records_inserted: i32,
    pub records_updated: i32,
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{
        CveIngestHealth, Health, HealthDiagnostics, PackageSourceEcosystem, PackageSourceExposure,
        PackageSourceExposureKevContext, PackageSourceExposures, PackageSourceExposuresStatus,
        UpgradeAssessmentCandidateVersion, UpgradeAssessmentCaveat, UpgradeAssessmentError,
        UpgradeAssessmentEvidence, UpgradeAssessmentEvidenceSourceType, UpgradeAssessmentFinding,
        UpgradeAssessmentFindingCategory, UpgradeAssessmentFindingEffect, UpgradeAssessmentInput,
        UpgradeAssessmentKevLinkage, UpgradeAssessmentPackageSourceInput, UpgradeAssessmentResult,
        UpgradeAssessmentStatus, UpgradeAssessmentVerdict, UpgradeAssessmentVulnerablePackageInput,
        UpgradeAssessmentWorkflowStatus, VersionedPackage,
    };
    use chrono::Utc;

    #[test]
    fn public_health_omits_operator_diagnostics() {
        let health = Health {
            status: "ok".to_owned(),
        };

        assert_eq!(
            serde_json::to_string(&health).unwrap(),
            r#"{"status":"ok"}"#
        );
    }

    #[test]
    fn operator_diagnostics_include_cve_ingest_state() {
        let health = HealthDiagnostics {
            status: "ok".to_owned(),
            cve_ingest: CveIngestHealth {
                records_available: true,
                latest_successful_commit: Some("abc123".to_owned()),
                latest_run: None,
            },
        };

        assert_eq!(
            serde_json::to_value(health).unwrap(),
            serde_json::json!({
                "status": "ok",
                "cveIngest": {
                    "recordsAvailable": true,
                    "latestSuccessfulCommit": "abc123",
                    "latestRun": null,
                },
            })
        );
    }

    #[test]
    fn upgrade_assessment_verdict_serializes_to_required_lowercase_values() {
        assert_eq!(
            serde_json::to_value(UpgradeAssessmentVerdict::Recommended).unwrap(),
            serde_json::json!("recommended")
        );
        assert_eq!(
            serde_json::to_value(UpgradeAssessmentVerdict::Caution).unwrap(),
            serde_json::json!("caution")
        );
        assert_eq!(
            serde_json::to_value(UpgradeAssessmentVerdict::Avoid).unwrap(),
            serde_json::json!("avoid")
        );
        assert_eq!(
            serde_json::to_value(UpgradeAssessmentVerdict::Unknown).unwrap(),
            serde_json::json!("unknown")
        );
    }

    #[test]
    fn upgrade_assessment_status_shape_serializes_as_workflow_metadata_only() {
        let timestamp = Utc::now();
        let status = UpgradeAssessmentStatus {
            id: uuid::Uuid::nil(),
            status: UpgradeAssessmentWorkflowStatus::Pending,
            created_at: timestamp,
            updated_at: timestamp,
            completed_at: None,
            error: None,
        };

        assert_eq!(
            serde_json::to_value(status).unwrap(),
            serde_json::json!({
                "id": uuid::Uuid::nil(),
                "status": "pending",
                "createdAt": timestamp,
                "updatedAt": timestamp,
            })
        );
    }

    #[test]
    fn upgrade_assessment_result_omits_optional_input_refinements_when_absent() {
        let timestamp = Utc::now();
        let result = UpgradeAssessmentResult {
            id: uuid::Uuid::nil(),
            status: UpgradeAssessmentWorkflowStatus::Completed,
            input: UpgradeAssessmentInput {
                package_source: UpgradeAssessmentPackageSourceInput {
                    ecosystem: PackageSourceEcosystem::Npm,
                    file_name: "package.json".to_owned(),
                    contents: "{}".to_owned(),
                },
                vulnerable_package: None,
                cve_linkage: None,
                kev_linkage: None,
                candidate_version: None,
            },
            verdict: UpgradeAssessmentVerdict::Unknown,
            summary: "Assessment queued".to_owned(),
            findings: Vec::new(),
            evidence: Vec::new(),
            caveats: Vec::new(),
            candidate_versions: Vec::<UpgradeAssessmentCandidateVersion>::new(),
            assessed_at: timestamp,
        };

        assert_eq!(
            serde_json::to_value(result).unwrap(),
            serde_json::json!({
                "id": uuid::Uuid::nil(),
                "status": "completed",
                "input": {
                    "packageSource": {
                        "ecosystem": "npm",
                        "fileName": "package.json",
                        "contents": "{}",
                    },
                },
                "verdict": "unknown",
                "summary": "Assessment queued",
                "findings": [],
                "evidence": [],
                "caveats": [],
                "candidateVersions": [],
                "assessedAt": timestamp,
            })
        );
    }

    #[test]
    fn upgrade_assessment_cve_and_kev_linkage_serialize_distinctly() {
        let input = UpgradeAssessmentInput {
            package_source: UpgradeAssessmentPackageSourceInput {
                ecosystem: PackageSourceEcosystem::Npm,
                file_name: "package-lock.json".to_owned(),
                contents: "{}".to_owned(),
            },
            vulnerable_package: Some(UpgradeAssessmentVulnerablePackageInput {
                name: "left-pad".to_owned(),
                ecosystem: PackageSourceEcosystem::Npm,
                version: "1.2.3".to_owned(),
                purl: Some("pkg:npm/left-pad@1.2.3".to_owned()),
            }),
            cve_linkage: Some(vec!["CVE-2026-1234".to_owned()]),
            kev_linkage: Some(UpgradeAssessmentKevLinkage {
                cve_ids: vec!["CVE-2026-1234".to_owned()],
                known_exploited: Some(true),
                references: vec![
                    "https://www.cisa.gov/known-exploited-vulnerabilities-catalog".to_owned(),
                ],
            }),
            candidate_version: Some("1.2.4".to_owned()),
        };

        assert_eq!(
            serde_json::to_value(input).unwrap(),
            serde_json::json!({
                "packageSource": {
                    "ecosystem": "npm",
                    "fileName": "package-lock.json",
                    "contents": "{}",
                },
                "vulnerablePackage": {
                    "name": "left-pad",
                    "ecosystem": "npm",
                    "version": "1.2.3",
                    "purl": "pkg:npm/left-pad@1.2.3",
                },
                "cveLinkage": ["CVE-2026-1234"],
                "kevLinkage": {
                    "cveIds": ["CVE-2026-1234"],
                    "knownExploited": true,
                    "references": ["https://www.cisa.gov/known-exploited-vulnerabilities-catalog"],
                },
                "candidateVersion": "1.2.4",
            })
        );
    }

    #[test]
    fn upgrade_assessment_result_supports_domain_finding_and_evidence_shapes() {
        let timestamp = Utc::now();
        let result = UpgradeAssessmentResult {
            id: uuid::Uuid::nil(),
            status: UpgradeAssessmentWorkflowStatus::Completed,
            input: UpgradeAssessmentInput {
                package_source: UpgradeAssessmentPackageSourceInput {
                    ecosystem: PackageSourceEcosystem::Npm,
                    file_name: "package.json".to_owned(),
                    contents: "{}".to_owned(),
                },
                vulnerable_package: None,
                cve_linkage: None,
                kev_linkage: None,
                candidate_version: None,
            },
            verdict: UpgradeAssessmentVerdict::Caution,
            summary: "Manual review is recommended.".to_owned(),
            findings: vec![UpgradeAssessmentFinding {
                id: "finding-1".to_owned(),
                category: UpgradeAssessmentFindingCategory::SupplyChain,
                effect: UpgradeAssessmentFindingEffect::Review,
                title: "Release delta requires review".to_owned(),
                summary: "The candidate upgrade crosses a major version boundary.".to_owned(),
                severity: Some("medium".to_owned()),
                confidence: Some("medium".to_owned()),
                evidence_ids: vec!["evidence-1".to_owned()],
            }],
            evidence: vec![UpgradeAssessmentEvidence {
                id: "evidence-1".to_owned(),
                source_type: UpgradeAssessmentEvidenceSourceType::Hipcheck,
                title: "Hipcheck assessment".to_owned(),
                summary: "Hipcheck reported compatibility-related concerns.".to_owned(),
                url: Some("/assessments/00000000-0000-0000-0000-000000000000/evidence".to_owned()),
                details: Some(serde_json::json!({ "check": "review" })),
                raw_source_identifiers: vec!["hipcheck-run-1".to_owned()],
            }],
            caveats: vec![UpgradeAssessmentCaveat {
                code: "candidate-not-verified".to_owned(),
                summary: "Candidate version was not directly verified as the fixing version."
                    .to_owned(),
            }],
            candidate_versions: Vec::new(),
            assessed_at: timestamp,
        };

        let value = serde_json::to_value(result).unwrap();
        assert_eq!(value["findings"][0]["category"], "supplyChain");
        assert_eq!(value["evidence"][0]["sourceType"], "hipcheck");
        assert_eq!(value["caveats"][0]["code"], "candidate-not-verified");
    }

    #[test]
    fn upgrade_assessment_error_omits_optional_code_when_absent() {
        let error = UpgradeAssessmentError {
            code: None,
            message: "assessment failed".to_owned(),
        };

        assert_eq!(
            serde_json::to_value(error).unwrap(),
            serde_json::json!({
                "message": "assessment failed",
            })
        );
    }

    #[test]
    fn package_source_exposures_completed_response_serializes_empty_results() {
        let response = PackageSourceExposures {
            id: uuid::Uuid::nil(),
            status: PackageSourceExposuresStatus::Completed,
            exposures: Vec::new(),
        };

        assert_eq!(
            serde_json::to_value(response).unwrap(),
            serde_json::json!({
                "id": uuid::Uuid::nil(),
                "status": "completed",
                "exposures": [],
            })
        );
    }

    #[test]
    fn package_source_exposures_include_reachable_package_cve_and_kev_context() {
        let response = PackageSourceExposures {
            id: uuid::Uuid::nil(),
            status: PackageSourceExposuresStatus::CompletedWithWarnings,
            exposures: vec![PackageSourceExposure {
                package: VersionedPackage {
                    id: uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000001")
                        .expect("valid UUID"),
                    name: "left-pad".to_owned(),
                    version: "1.2.3".to_owned(),
                    ecosystem: PackageSourceEcosystem::Npm,
                    purl: "pkg:npm/left-pad@1.2.3".to_owned(),
                    derivations: vec![vec![
                        "pkg:npm/example-app@1.0.0".to_owned(),
                        "pkg:npm/left-pad@1.2.3".to_owned(),
                    ]],
                },
                cve_id: "CVE-2026-1234".to_owned(),
                kev: PackageSourceExposureKevContext {
                    vendor_project: Some("Example Vendor".to_owned()),
                    product: Some("left-pad".to_owned()),
                    vulnerability_name: Some("Example vulnerability".to_owned()),
                    date_added: Some("2026-08-03".to_owned()),
                },
            }],
        };

        assert_eq!(
            serde_json::to_value(response).unwrap(),
            serde_json::json!({
                "id": uuid::Uuid::nil(),
                "status": "completed-with-warnings",
                "exposures": [
                    {
                        "package": {
                            "id": "00000000-0000-0000-0000-000000000001",
                            "name": "left-pad",
                            "version": "1.2.3",
                            "ecosystem": "npm",
                            "purl": "pkg:npm/left-pad@1.2.3",
                            "derivations": [[
                                "pkg:npm/example-app@1.0.0",
                                "pkg:npm/left-pad@1.2.3"
                            ]]
                        },
                        "cveId": "CVE-2026-1234",
                        "kev": {
                            "vendorProject": "Example Vendor",
                            "product": "left-pad",
                            "vulnerabilityName": "Example vulnerability",
                            "dateAdded": "2026-08-03"
                        }
                    }
                ]
            })
        );
    }
}

#[derive(Deserialize, JsonSchema, Debug, Clone)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
#[schemars(deny_unknown_fields)]
pub struct PostPackageSourceBody {
    pub file_name: String,
    pub contents: String,
}

#[derive(Serialize, JsonSchema, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PostPackageSourceResponse {
    pub id: Uuid,
}

#[derive(Serialize, JsonSchema, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PackageSourceOperationResponse {
    pub id: Uuid,
    pub status: PackageSourceOperationStatus,
}

#[derive(Serialize, JsonSchema, Debug, Clone)]
#[serde(rename_all = "kebab-case")]
pub enum PackageSourceOperationStatus {
    Cancelled,
    Deleting,
}

#[derive(Deserialize, JsonSchema, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PackageSourcePathParams {
    pub id: Uuid,
}

#[derive(Serialize, JsonSchema, Debug, Clone)]
#[serde(tag = "status")]
#[serde(rename_all = "camelCase")]
pub enum PackageSourceStatus {
    Processing(PackageSourceStatusProcessing),
    Cancelled(PackageSourceStatusCancelled),
    Completed(PackageSourceStatusCompleted),
    #[serde(rename = "completed-with-warnings")]
    CompletedWithWarnings(PackageSourceStatusCompletedWithWarnings),
    Failed(PackageSourceStatusFailed),
}

#[derive(Serialize, JsonSchema, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PackageSourceStatusProcessing {
    pub id: Uuid,
    // We'd prefer to use `jiff` over `chrono`, but `dropshot` depends on
    // an old version of `schemars` that doesn't support `jiff`. When we
    // can use a newer version of `schemars`, we should switch to using
    // `jiff`.
    pub created_at: DateTime<Utc>,
    pub attempt: i32,
}

#[derive(Serialize, JsonSchema, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PackageSourceStatusCancelled {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub cancelled_at: DateTime<Utc>,
    pub attempt: i32,
}

#[derive(Serialize, JsonSchema, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PackageSourceStatusCompleted {
    pub id: Uuid,
    // We'd prefer to use `jiff` over `chrono`, but `dropshot` depends on
    // an old version of `schemars` that doesn't support `jiff`. When we
    // can use a newer version of `schemars`, we should switch to using
    // `jiff`.
    pub created_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    pub attempt: i32,
    pub source: PackageSource,
    pub versioned_packages: Vec<VersionedPackage>,
}

#[derive(Serialize, JsonSchema, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PackageSourceStatusCompletedWithWarnings {
    pub id: Uuid,
    // We'd prefer to use `jiff` over `chrono`, but `dropshot` depends on
    // an old version of `schemars` that doesn't support `jiff`. When we
    // can use a newer version of `schemars`, we should switch to using
    // `jiff`.
    pub created_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    pub attempt: i32,
    pub source: PackageSource,
    pub versioned_packages: Vec<VersionedPackage>,
    pub warnings: Vec<PackageSourceWarning>,
    pub warnings_truncated: bool,
}

#[derive(Serialize, JsonSchema, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PackageSourceStatusFailed {
    pub id: Uuid,
    // We'd prefer to use `jiff` over `chrono`, but `dropshot` depends on
    // an old version of `schemars` that doesn't support `jiff`. When we
    // can use a newer version of `schemars`, we should switch to using
    // `jiff`.
    pub created_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub attempt: i32,
    pub diagnostic: String,
    /// The last successfully published reachable-version snapshot, if any.
    ///
    /// A failed re-elaboration does not replace the prior snapshot, so callers
    /// can continue to inspect it while acting on `diagnostic`.
    pub previous_versioned_packages: Vec<VersionedPackage>,
}

#[derive(Serialize, JsonSchema, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PackageSourceWarning {
    pub declared_by_purl: Option<String>,
    pub dependency_name: String,
    pub specification_kind: String,
    /// A fixed explanation that does not include untrusted registry content.
    pub message: String,
}

#[derive(Serialize, JsonSchema, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PackageSource {
    pub ecosystem: PackageSourceEcosystem,
    pub file_name: String,
    pub contents: String,
}

#[derive(Serialize, JsonSchema, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct VersionedPackage {
    pub id: Uuid,
    pub name: String,
    pub version: String,
    pub ecosystem: PackageSourceEcosystem,
    pub purl: String,
    pub derivations: Vec<Vec<String>>,
}

#[derive(Serialize, JsonSchema, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PackageSourceExposuresStatus {
    Completed,
    CompletedWithWarnings,
    Failed,
}

#[derive(Serialize, JsonSchema, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PackageSourceExposures {
    pub id: Uuid,
    pub status: PackageSourceExposuresStatus,
    pub exposures: Vec<PackageSourceExposure>,
}

#[derive(Serialize, JsonSchema, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PackageSourceExposure {
    pub package: VersionedPackage,
    pub cve_id: String,
    pub kev: PackageSourceExposureKevContext,
}

#[derive(Serialize, JsonSchema, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PackageSourceExposureKevContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vendor_project: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vulnerability_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_added: Option<String>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub enum PackageSourceEcosystem {
    Npm,
}

#[derive(Deserialize, Serialize, JsonSchema, Debug, Clone)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
#[schemars(deny_unknown_fields)]
pub struct UpgradeAssessmentInput {
    pub package_source: UpgradeAssessmentPackageSourceInput,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vulnerable_package: Option<UpgradeAssessmentVulnerablePackageInput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cve_linkage: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kev_linkage: Option<UpgradeAssessmentKevLinkage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidate_version: Option<String>,
}

/// Request body for creating a new asynchronous upgrade assessment.
pub type PostUpgradeAssessmentBody = UpgradeAssessmentInput;

#[derive(Serialize, JsonSchema, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PostUpgradeAssessmentResponse {
    pub id: Uuid,
    pub status: UpgradeAssessmentWorkflowStatus,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_url: Option<String>,
}

#[derive(Deserialize, JsonSchema, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeAssessmentPathParams {
    pub id: Uuid,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum UpgradeAssessmentWorkflowStatus {
    Pending,
    Completed,
    Failed,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeAssessmentStatus {
    pub id: Uuid,
    pub status: UpgradeAssessmentWorkflowStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<UpgradeAssessmentError>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeAssessmentResult {
    pub id: Uuid,
    pub status: UpgradeAssessmentWorkflowStatus,
    pub input: UpgradeAssessmentInput,
    pub verdict: UpgradeAssessmentVerdict,
    pub summary: String,
    pub findings: Vec<UpgradeAssessmentFinding>,
    pub evidence: Vec<UpgradeAssessmentEvidence>,
    pub caveats: Vec<UpgradeAssessmentCaveat>,
    pub candidate_versions: Vec<UpgradeAssessmentCandidateVersion>,
    pub assessed_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeAssessmentPackageSourceInput {
    pub ecosystem: PackageSourceEcosystem,
    pub file_name: String,
    pub contents: String,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeAssessmentVulnerablePackageInput {
    pub name: String,
    pub ecosystem: PackageSourceEcosystem,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub purl: Option<String>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeAssessmentKevLinkage {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cve_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub known_exploited: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<String>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum UpgradeAssessmentVerdict {
    Recommended,
    Caution,
    Avoid,
    Unknown,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeAssessmentCandidateVersion {
    pub version: String,
    pub is_requested_candidate: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_timestamp: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upgrade_distance: Option<UpgradeAssessmentUpgradeDistance>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verdict: Option<UpgradeAssessmentVerdict>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum UpgradeAssessmentUpgradeDistance {
    Patch,
    Minor,
    Major,
    Unknown,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum UpgradeAssessmentFindingCategory {
    Vulnerability,
    SupplyChain,
    MissingEvidence,
    CaveatTrigger,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum UpgradeAssessmentFindingEffect {
    Blocking,
    Review,
    Context,
    MissingCheck,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeAssessmentFinding {
    pub id: String,
    pub category: UpgradeAssessmentFindingCategory,
    pub effect: UpgradeAssessmentFindingEffect,
    pub title: String,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_ids: Vec<String>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum UpgradeAssessmentEvidenceSourceType {
    VulnerabilityDatabase,
    Kev,
    Hipcheck,
    PackageRegistry,
    InternalAnalysis,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeAssessmentEvidence {
    pub id: String,
    pub source_type: UpgradeAssessmentEvidenceSourceType,
    pub title: String,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub raw_source_identifiers: Vec<String>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeAssessmentCaveat {
    pub code: String,
    pub summary: String,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeAssessmentError {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    pub message: String,
}
