//! Defines the API endpoints for the Night Vision server.

pub mod ctx;

use crate::{api::ctx::ApiCtx, config::Config, env::Env, error::FatalError, log::logger};
use dropshot::{HttpError, HttpResponseOk, RequestContext, ServerBuilder};
use nv_server_api::{Health, NvServerApi, nv_server_api_mod::api_description};

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
