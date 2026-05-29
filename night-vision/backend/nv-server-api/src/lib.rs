use dropshot::{HttpError, HttpResponseOk, RequestContext};
use schemars::JsonSchema;
use serde::Serialize;

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
}

#[derive(Serialize, JsonSchema)]
pub struct Health {
    pub status: String,
}
