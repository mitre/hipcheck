//! Defines the API endpoints for the Night Vision server.

use crate::error::FatalError;
use dropshot::{HttpError, HttpResponseOk, RequestContext};
use nv_server_api::{
    NvServerApi, Stub, nv_server_api_mod::api_description as generate_api_description,
};
use sea_orm::DatabaseConnection;

pub struct Api;

impl Api {
    /// Create a `dropshot::ApiDescription` based on `Api`.
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Result<dropshot::ApiDescription<ApiCtx>, FatalError> {
        // PANIC SAFETY: Since we've implemented the required trait, this function should succeed.
        generate_api_description::<Api>().map_err(FatalError::FailedToBuildDropshotServer)
    }
}

impl NvServerApi for Api {
    type Context = ApiCtx;

    async fn example_endpoint(
        _ctx: RequestContext<Self::Context>,
    ) -> Result<HttpResponseOk<Stub>, HttpError> {
        Ok(HttpResponseOk(Stub {
            name: "example".to_string(),
        }))
    }
}

pub struct ApiCtx {
    #[allow(unused)]
    pub db: DatabaseConnection,
}
