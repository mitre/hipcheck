// We'd prefer to use `jiff` over `chrono`, but `dropshot` depends on
// an old version of `schemars` that doesn't support `jiff`. When we
// can use a newer version of `schemars`, we should switch to using
// `jiff`.
use chrono::{DateTime, Utc};
use dropshot::{
    HttpError, HttpResponseAccepted, HttpResponseOk, Path, RequestContext, TypedBody, UntypedBody,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Maximum encoded JSON request size for a package-source submission.
///
/// This accommodates a maximally escaped 1 MiB `package.json` document plus
/// the JSON request envelope.
pub const MAX_PACKAGE_SOURCE_REQUEST_BODY_BYTES: usize = 3 * 1024 * 1024;

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
        body_param: UntypedBody,
    ) -> Result<HttpResponseAccepted<PostPackageSourceResponse>, HttpError>;

    #[endpoint {
        method = GET,
        path = "/package-sources/{id}",
    }]
    async fn get_package_source(
        ctx: RequestContext<Self::Context>,
        path_params: Path<PackageSourcePathParams>,
    ) -> Result<HttpResponseOk<PackageSourceStatus>, HttpError>;

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
    use super::{CveIngestHealth, Health, HealthDiagnostics};

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

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub enum PackageSourceEcosystem {
    Npm,
}

/// Input for an asynchronous NPM upgrade-safety assessment.
#[derive(Deserialize, JsonSchema, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PostUpgradeAssessmentBody {
    pub package_name: String,
    pub current_version: String,
    pub trigger: UpgradeAssessmentTrigger,
    pub candidate_version: Option<String>,
}

/// The vulnerability or previously-created exposure that caused this assessment.
#[derive(Deserialize, Serialize, JsonSchema, Debug, Clone)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum UpgradeAssessmentTrigger {
    Cve { cve_id: String },
    Exposure { exposure_id: String },
}

#[derive(Serialize, JsonSchema, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PostUpgradeAssessmentResponse {
    pub id: Uuid,
}

#[derive(Deserialize, JsonSchema, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeAssessmentPathParams {
    pub id: Uuid,
}

/// The durable lifecycle state of an upgrade assessment.
#[derive(Serialize, JsonSchema, Debug, Clone)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum UpgradeAssessmentStatus {
    Processing(UpgradeAssessmentProcessing),
    Completed(UpgradeAssessmentCompleted),
    Failed(UpgradeAssessmentFailed),
}

#[derive(Serialize, JsonSchema, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeAssessmentProcessing {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub input: UpgradeAssessmentInput,
}

#[derive(Serialize, JsonSchema, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeAssessmentCompleted {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    pub report: UpgradeAssessmentReport,
}

#[derive(Serialize, JsonSchema, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeAssessmentFailed {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub failed_at: DateTime<Utc>,
    pub input: UpgradeAssessmentInput,
    pub error: String,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeAssessmentInput {
    pub ecosystem: PackageSourceEcosystem,
    pub package_name: String,
    pub current_version: String,
    pub trigger: UpgradeAssessmentTrigger,
    pub candidate_version: Option<String>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeAssessmentReport {
    pub input: UpgradeAssessmentInput,
    pub verdict: UpgradeAssessmentVerdict,
    pub candidate_versions: Vec<UpgradeAssessmentCandidate>,
    pub vulnerability_context: UpgradeAssessmentVulnerabilityContext,
    pub dependency_delta: UpgradeAssessmentDependencyDelta,
    pub supply_chain_findings: Vec<UpgradeAssessmentFinding>,
    pub confidence: UpgradeAssessmentConfidence,
    pub caveats: Vec<String>,
    pub evidence_links: Vec<String>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub enum UpgradeAssessmentVerdict {
    Recommended,
    Caution,
    Avoid,
    Unknown,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub enum UpgradeAssessmentConfidence {
    High,
    Medium,
    Low,
    Unknown,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeAssessmentCandidate {
    pub version: String,
    #[serde(default)]
    pub upgrade_distance: UpgradeAssessmentUpgradeDistance,
    #[serde(default)]
    pub api_compatibility: UpgradeAssessmentApiCompatibility,
    pub verdict: UpgradeAssessmentVerdict,
    pub caveats: Vec<String>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum UpgradeAssessmentUpgradeDistance {
    Patch,
    Minor,
    Major,
    #[default]
    Unknown,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, Default, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum UpgradeAssessmentApiCompatibility {
    Compatible,
    Incompatible,
    NoGuarantee,
    #[default]
    Unknown,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeAssessmentVulnerabilityContext {
    pub trigger: UpgradeAssessmentTrigger,
    pub kev_linked: Option<bool>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeAssessmentDependencyDelta {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub changed: Vec<String>,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeAssessmentFinding {
    pub effect: String,
    pub summary: String,
    pub evidence_links: Vec<String>,
}
