// We'd prefer to use `jiff` over `chrono`, but `dropshot` depends on
// an old version of `schemars` that doesn't support `jiff`. When we
// can use a newer version of `schemars`, we should switch to using
// `jiff`.
use chrono::{DateTime, Utc};
use dropshot::{HttpError, HttpResponseAccepted, HttpResponseOk, Path, RequestContext, TypedBody};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
}

#[derive(Serialize, JsonSchema)]
pub struct Health {
    pub status: String,
}

#[derive(Deserialize, JsonSchema, Debug, Clone)]
#[serde(rename_all = "camelCase")]
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
    pub derivation: Vec<String>,
}

#[derive(Serialize, JsonSchema, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub enum PackageSourceEcosystem {
    Npm,
}
