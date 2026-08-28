//! Defines the API endpoints for the Night Vision server.

pub mod ctx;

mod cve_worker;

use crate::{api::ctx::ApiCtx, error::FatalError};
use camino::{Utf8Path, Utf8PathBuf};
// We'd prefer to use `jiff` over `chrono`, but `dropshot` depends on
// an old version of `schemars` that doesn't support `jiff`. When we
// can use a newer version of `schemars`, we should switch to using
// `jiff`.
use chrono::{TimeZone as _, Utc};
use dropshot::{
    ClientErrorStatusCode, HttpError, HttpResponseAccepted, HttpResponseOk, Path, RequestContext,
    ServerBuilder, UntypedBody,
};
use nv_common::{
    config::Config,
    cve::storage::{
        has_cve_list_records, last_successful_cve_list_sync_commit, latest_cve_list_sync_run,
    },
    db::entities::cve_list_sync_runs::Model as CveListSyncRun,
};
use nv_server_api::{
    CveIngestHealth, CveListSyncRunHealth, Health, NvServerApi, PackageSource,
    PackageSourceEcosystem, PackageSourcePathParams, PackageSourceStatus,
    PackageSourceStatusCompleted, PackageSourceStatusProcessing, PostPackageSourceBody,
    PostPackageSourceResponse, VersionedPackage, nv_server_api_mod::api_description,
};
use sea_orm::DatabaseConnection;
use slog::Logger;
use std::fs::File;
use uuid::Uuid;

/// The REST API interface.
///
/// This is a wrapper for the `dropshot` `ApiDescription` populate with our `ApiCtx` (the shared
/// data available to every endpoint handler).
pub struct RestApi(dropshot::ApiDescription<ApiCtx>);

impl RestApi {
    /// Initialize the API.
    pub fn new() -> Result<Self, FatalError> {
        api_description::<Self>()
            .map(RestApi)
            .map_err(FatalError::FailedToBuildDropshotServer)
    }

    /// Launch the server, handling requests.
    ///
    /// Returns `FatalError` if the server encounters a problem that either blocks launching or
    /// causes the server to be unable to serve more requests.
    pub async fn serve(self, config: &Config, log: Logger) -> Result<(), FatalError> {
        let api = self.0;
        let ctx = ApiCtx::init(config).await?;
        let cve_list_worker_config = config.cve_list_worker_config();
        let _cve_list_worker = cve_worker::spawn_cve_list_worker(
            ctx.db().clone(),
            cve_list_worker_config,
            log.clone(),
        );
        let _kev_worker =
            nv_common::kev::spawn_kev_sync_worker(config, ctx.db().clone(), log.clone());

        ServerBuilder::new(api, ctx, log)
            .config(config.dropshot_config()?)
            .start()
            .map_err(FatalError::FailedToStartDropshotServer)?
            .await
            .map_err(FatalError::UnknownServerError)
    }

    /// Generate an OpenAPI Description from the
    /// dropshot::ApiDescription and write it out to the specified path.
    /// If the destination path argument is an absolute path, it is taken as-is.
    /// If it is a relative path, it is interpreted as relative to the
    /// location of the workspace root on the filesystem.
    /// The OpenAPI Description will include the current version of
    /// the `nv-server` crate.
    pub fn write_openapi(&self, dest_path: &Utf8Path) -> Result<(), FatalError> {
        let ver_string = env!("CARGO_PKG_VERSION");
        let ver = dropshot::semver::Version::parse(ver_string)
            // PANIC SAFETY: Cargo enforces semver version syntax to build, so this parse should always succeed
            .expect("Internal error: failed to parse nv-server version as semver format");
        let openapi = self.0.openapi("Night Vision", ver);

        let path = openapi_dest_path(dest_path);
        // Explicitly match on the Result instead of using `.map_err`,
        // to avoid needing to clone `path` to pass to two error
        // closures. Currently, Rust can't prove that `path` would
        // only be used once in each case, if the first path involves
        // passing `path` to a closure.
        let mut file = match File::create(&path) {
            Ok(file) => file,
            Err(e) => return Err(FatalError::FailedToCreateOpenApiDescFile(path, e)),
        };

        openapi
            .write(&mut file)
            .map_err(|e| FatalError::FailedToWriteOpenApiDescFile(path, e))?;
        Ok(())
    }
}

/// Create the path to store an OpenAPI Description.
/// If the destination path argument is an absolute path, it is taken as-is.
/// If it is a relative path, it is interpreted as relative to the
/// location of the workspace root on the filesystem.
fn openapi_dest_path(dest_path: &Utf8Path) -> Utf8PathBuf {
    // This directory path will point to "backend/nv-server"
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let mut path = Utf8PathBuf::new();
    path.push(manifest_dir);
    // Pop the trailing `/nv-server`, to make the path relative to the
    // workspace root
    path.pop();
    // This call to `push()` may either replace the entire path (if
    // `dest_path` is absolute) or append `dest_path` (if relative)
    path.push(dest_path);

    path
}

// Implementation of the `NvServerApi` trait. This is where our actual endpoint handlers go.
//
// Note that the trait and relevant API types are defined in the `nv-server-api` crate.
impl NvServerApi for RestApi {
    type Context = ApiCtx;

    async fn health(
        ctx: RequestContext<Self::Context>,
    ) -> Result<HttpResponseOk<Health>, HttpError> {
        let cve_ingest = cve_ingest_health(ctx.context().db()).await?;

        Ok(HttpResponseOk(Health {
            status: "ok".to_owned(),
            cve_ingest,
        }))
    }

    async fn post_package_source(
        ctx: RequestContext<Self::Context>,
        body_param: UntypedBody,
    ) -> Result<HttpResponseAccepted<PostPackageSourceResponse>, HttpError> {
        validate_package_source_media_type(
            ctx.request
                .headers()
                .get("content-type")
                .and_then(|value| value.to_str().ok()),
        )?;
        let body = parse_package_source_body(body_param.as_bytes())?;

        // Retain this permit until the request is accepted. This bounds both
        // manifest parsing and future persistence work per server process.
        let _submission_slot = ctx.context().try_acquire_package_source_submission_slot()?;
        let db = ctx.context().db();
        let file_name = body.file_name;
        let contents = body.contents;

        validate_package_source(
            &file_name,
            &contents,
            ctx.context().package_source_contents_max_bytes(),
        )?;
        let uuid = tokio::time::timeout(
            ctx.context().package_source_request_timeout(),
            store_package_source(db, file_name, contents),
        )
        .await
        .map_err(|_| {
            HttpError::for_unavail(
                Some("PackageSourceRequestTimedOut".to_owned()),
                "package-source submission timed out".to_owned(),
            )
        })??;
        let resp = HttpResponseAccepted(PostPackageSourceResponse { id: uuid });

        Ok(resp)
    }

    async fn get_package_source(
        ctx: RequestContext<Self::Context>,
        path_params: Path<PackageSourcePathParams>,
    ) -> Result<HttpResponseOk<PackageSourceStatus>, HttpError> {
        let db = ctx.context().db();
        let id = path_params.into_inner().id;
        let pkg_src = lookup_package_source(db, id).await?;
        match pkg_src {
            Some(status) => Ok(HttpResponseOk(status)),
            None => Err(HttpError::for_not_found(
                None,
                format!("unknown package source {id}"),
            )),
        }
    }
}

fn validate_package_source_media_type(content_type: Option<&str>) -> Result<(), HttpError> {
    let Some(content_type) = content_type else {
        return Err(unsupported_package_source_media_type());
    };
    let media_type = content_type
        .split_once(';')
        .map_or(content_type, |(media_type, _)| media_type)
        .trim();

    if media_type.eq_ignore_ascii_case("application/json") {
        Ok(())
    } else {
        Err(unsupported_package_source_media_type())
    }
}

fn unsupported_package_source_media_type() -> HttpError {
    HttpError::for_client_error(
        Some("UnsupportedPackageSourceMediaType".to_owned()),
        ClientErrorStatusCode::UNSUPPORTED_MEDIA_TYPE,
        "package-source requests must use application/json".to_owned(),
    )
}

fn parse_package_source_body(contents: &[u8]) -> Result<PostPackageSourceBody, HttpError> {
    serde_json::from_slice(contents).map_err(|_| {
        HttpError::for_bad_request(
            Some("InvalidPackageSourceRequest".to_owned()),
            "package-source request body must be valid JSON".to_owned(),
        )
    })
}

fn validate_package_source(
    file_name: &str,
    contents: &str,
    max_contents_bytes: usize,
) -> Result<(), HttpError> {
    if file_name != "package.json" {
        return Err(HttpError::for_bad_request(
            Some("InvalidPackageSourceFileName".to_owned()),
            "fileName must be exactly \"package.json\"".to_owned(),
        ));
    }

    if contents.len() > max_contents_bytes {
        return Err(HttpError::for_client_error(
            Some("PackageSourceContentsTooLarge".to_owned()),
            ClientErrorStatusCode::PAYLOAD_TOO_LARGE,
            format!("contents must not exceed {max_contents_bytes} bytes"),
        ));
    }

    match nv_common::npm::package_json::NpmPackageJson::parse_package_json(contents.as_bytes()) {
        Ok(_) => {}
        Err(nv_common::npm::package_json::PackageParseError::PackageTooLarge) => {
            return Err(HttpError::for_client_error(
                Some("PackageSourceContentsTooLarge".to_owned()),
                ClientErrorStatusCode::PAYLOAD_TOO_LARGE,
                "contents exceeds the supported package.json size limit".to_owned(),
            ));
        }
        Err(_) => {
            return Err(HttpError::for_bad_request(
                Some("InvalidPackageSourceContents".to_owned()),
                "contents must be a valid npm package.json document".to_owned(),
            ));
        }
    }

    Ok(())
}

/// Placeholder implementation of storing a submitted Package Source
/// to persistent storage.
/// TODO implement it via real database calls.
async fn store_package_source(
    _db: &DatabaseConnection,
    _file_name: String,
    _contents: String,
) -> Result<Uuid, HttpError> {
    // TODO replace this hardcoded value
    let uuid_a = Uuid::from_u64_pair(0, 1);
    Ok(uuid_a)
}

/// Placeholder implementation of looking up the status of a Package Source
/// from persistent storage.
/// TODO implement it via real database calls.
async fn lookup_package_source(
    _db: &DatabaseConnection,
    id: Uuid,
) -> Result<Option<PackageSourceStatus>, HttpError> {
    use std::collections::HashMap;
    // TODO replace these hardcoded values
    let uuid_a = Uuid::from_u64_pair(0, 1);
    let uuid_b = Uuid::from_u64_pair(0, 2);
    let uuid_c = Uuid::from_u64_pair(0, 3);

    // TODO replace this hardcoded value
    let dt = Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap();

    // TODO replace these hardcoded values
    let fake_statuses: HashMap<Uuid, PackageSourceStatus> = HashMap::from([
        (
            uuid_a,
            PackageSourceStatus::Processing(PackageSourceStatusProcessing {
                id: uuid_a,
                created_at: dt,
            }),
        ),
        (
            uuid_b,
            PackageSourceStatus::Completed(PackageSourceStatusCompleted {
                id: uuid_b,
                created_at: dt,
                source: PackageSource {
                    ecosystem: PackageSourceEcosystem::Npm,
                    file_name: "package.json".to_owned(),
                    contents: "{}".to_owned(),
                },
                versioned_packages: vec![VersionedPackage {
                    id: uuid_c,
                    name: "react".to_owned(),
                    version: "18.2.0".to_owned(),
                    ecosystem: PackageSourceEcosystem::Npm,
                    purl: "pkg:npm/react@18.2.0".to_owned(),
                    derivation: vec!["<root>".to_owned(), "pkg:npm/react@18.2.0".to_owned()],
                }],
            }),
        ),
    ]);

    let status = fake_statuses.get(&id).cloned();
    Ok(status)
}

async fn cve_ingest_health(db: &DatabaseConnection) -> Result<CveIngestHealth, HttpError> {
    let records_available = has_cve_list_records(db)
        .await
        .map_err(|error| HttpError::for_internal_error(error.to_string()))?;
    let latest_successful_commit = last_successful_cve_list_sync_commit(db)
        .await
        .map_err(|error| HttpError::for_internal_error(error.to_string()))?
        .map(|commit| commit.as_str().to_owned());
    let latest_run = latest_cve_list_sync_run(db)
        .await
        .map_err(|error| HttpError::for_internal_error(error.to_string()))?
        .map(cve_list_sync_run_health);

    Ok(CveIngestHealth {
        records_available,
        latest_successful_commit,
        latest_run,
    })
}

fn cve_list_sync_run_health(run: CveListSyncRun) -> CveListSyncRunHealth {
    CveListSyncRunHealth {
        generation: run.generation,
        status: run.status,
        checked_at: run.checked_at.with_timezone(&Utc),
        completed_at: run
            .completed_at
            .map(|completed_at| completed_at.with_timezone(&Utc)),
        records_seen: run.records_seen,
        records_inserted: run.records_inserted,
        records_updated: run.records_updated,
        error: run.error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONTENTS_MAX_BYTES: usize = 1024;

    #[test]
    fn package_source_body_rejects_unknown_fields() {
        let _error = parse_package_source_body(
            br#"{\"fileName\":\"package.json\",\"contents\":\"{}\",\"extra\":true}"#,
        )
        .expect_err("unknown request fields should be rejected");
    }

    #[test]
    fn package_source_media_type_requires_json() {
        let _error = validate_package_source_media_type(Some("text/plain"))
            .expect_err("non-JSON media types should be rejected");
        validate_package_source_media_type(Some("application/json; charset=utf-8"))
            .expect("JSON media types should be accepted");
    }

    #[test]
    fn package_source_validation_rejects_noncanonical_file_names() {
        let _error = validate_package_source("../package.json", "{}", CONTENTS_MAX_BYTES)
            .expect_err("noncanonical file names should be rejected");
    }

    #[test]
    fn package_source_validation_rejects_malformed_manifest_contents() {
        let _error = validate_package_source("package.json", "{", CONTENTS_MAX_BYTES)
            .expect_err("malformed manifest contents should be rejected");
    }

    #[test]
    fn package_source_validation_rejects_oversized_contents() {
        let contents = "x".repeat(CONTENTS_MAX_BYTES + 1);
        let _error = validate_package_source("package.json", &contents, CONTENTS_MAX_BYTES)
            .expect_err("oversized manifest contents should be rejected");
    }
}
