use dropshot::{HttpError, HttpResponseOk, RequestContext};
use schemars::JsonSchema;
use serde::Serialize;

#[dropshot::api_description]
pub trait NvServerApi {
    type Context: Send + Sync + 'static;

    #[endpoint {
        method = GET,
        path = "/example_endpoint",
    }]
    async fn example_endpoint(
        ctx: RequestContext<Self::Context>,
    ) -> Result<HttpResponseOk<Stub>, HttpError>;
}

#[derive(Serialize, JsonSchema)]
pub struct Stub {
    pub name: String,
}
