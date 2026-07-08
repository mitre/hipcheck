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
    HttpError, HttpResponseAccepted, HttpResponseOk, Path, RequestContext, ServerBuilder, TypedBody,
};
use nv_common::config::Config;
use nv_server_api::{
    Health, NvServerApi, PackageSource, PackageSourceEcosystem, PackageSourcePathParams,
    PackageSourceStatus, PackageSourceStatusCompleted, PackageSourceStatusProcessing,
    PostPackageSourceBody, PostPackageSourceResponse, VersionedPackage,
    nv_server_api_mod::api_description,
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
        cve_worker::sync_cve_list_on_startup(ctx.db(), &cve_list_worker_config, &log).await?;
        let _cve_list_worker = cve_worker::spawn_cve_list_worker(
            ctx.db().clone(),
            cve_list_worker_config,
            log.clone(),
        );

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
        _ctx: RequestContext<Self::Context>,
    ) -> Result<HttpResponseOk<Health>, HttpError> {
        Ok(HttpResponseOk(Health {
            status: "ok".to_owned(),
        }))
    }

    async fn post_package_source(
        ctx: RequestContext<Self::Context>,
        body_param: TypedBody<PostPackageSourceBody>,
    ) -> Result<HttpResponseAccepted<PostPackageSourceResponse>, HttpError> {
        let db = ctx.context().db();
        let body = body_param.into_inner();
        let file_name = body.file_name;
        let contents = body.contents;

        let uuid = store_package_source(db, file_name, contents).await?;
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
