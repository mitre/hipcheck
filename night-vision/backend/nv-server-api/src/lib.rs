// We'd prefer to use `jiff` over `chrono`, but `dropshot` depends on
// an old version of `schemars` that doesn't support `jiff`. When we
// can use a newer version of `schemars`, we should switch to using
// `jiff`.
use chrono::{DateTime, Utc};
use dropshot::{
    HttpError, HttpResponseAccepted, HttpResponseOk, Path, RequestContext, UntypedBody,
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
        body_param: UntypedBody,
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
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PostAssessmentBody {
    pub purl: String,
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

#[derive(Serialize, JsonSchema, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub enum PackageSourceEcosystem {
    Npm,
}
