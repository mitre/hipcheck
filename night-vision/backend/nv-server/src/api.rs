use dropshot::{HttpError, HttpResponseOk, RequestContext};
use nv_server_api::nv_server_api_mod::api_description as generate_api_description;
use nv_server_api::{NvServerApi, Stub};

use crate::context::ApiCtx;

pub struct Api;

impl Api {
    /// Create a `dropshot::ApiDescription` based on `Api`.
    pub fn new() -> dropshot::ApiDescription<ApiCtx> {
        // PANIC SAFETY: Since we've implemented the required trait, this function should succeed.
        generate_api_description::<Api>().expect("failed to register API")
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
