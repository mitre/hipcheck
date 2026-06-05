//! Defines the API endpoints for the Night Vision server.

pub mod ctx;

use crate::{api::ctx::ApiCtx, config::Config, env::Env, error::FatalError, log::logger};
use camino::{Utf8Path, Utf8PathBuf};
use dropshot::{HttpError, HttpResponseOk, RequestContext, ServerBuilder};
use nv_server_api::{Health, NvServerApi, nv_server_api_mod::api_description};
use std::fs::File;

/// The REST API interface.
///
/// This is a wrapper for the `dropshot` `ApiDescription` populate with our `ApiCtx` (the shared
/// data available to every endpoint handler).
pub struct RestApi(dropshot::ApiDescription<ApiCtx>);

impl RestApi {
    /// Initialize the API.
    pub fn new() -> Result<Self, FatalError> {
        api_description::<RestApi>()
            .map(RestApi)
            .map_err(FatalError::FailedToBuildDropshotServer)
    }

    /// Launch the server, handling requests.
    ///
    /// Returns `FatalError` if the server encounters a problem that either blocks launching or
    /// causes the server to be unable to serve more requests.
    pub async fn serve(self, env: &Env, config: &Config) -> Result<(), FatalError> {
        let api = self.0;
        let ctx = ApiCtx::init(config).await?;
        let log = logger(env)?;

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
        // This silences an unused warning on `ApiCtx::db`, which we need for now.
        let _db = ctx.context().db();

        Ok(HttpResponseOk(Health {
            status: "ok".to_string(),
        }))
    }
}
