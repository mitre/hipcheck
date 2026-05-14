//! Defines the API endpoints for the Night Vision server.

use crate::{config::Config, env::Env, error::FatalError};
use dropshot::{
    ConfigLogging, ConfigLoggingLevel, HttpError, HttpResponseOk, RequestContext, ServerBuilder,
};
use nv_server_api::{NvServerApi, Stub, nv_server_api_mod::api_description};
use sea_orm::DatabaseConnection;

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
        let ctx = ApiCtx::init(&config).await?;

        // TODO: Factor logger-setup into its own function. Currently waiting on dropshot#1607 [1].
        //
        // [1]: https://github.com/oxidecomputer/dropshot/issues/1607
        let log = ConfigLogging::StderrTerminal {
            level: ConfigLoggingLevel::Info,
        }
        .to_logger(env.bin_name())
        .map_err(FatalError::FailedToInitializeLogger)?;

        ServerBuilder::new(api, ctx, log)
            .config(config.dropshot_config()?)
            .start()
            .map_err(FatalError::FailedToStartDropshotServer)?
            .await
            .map_err(FatalError::UnknownServerError)
    }
}

// Implementation of the `NvServerApi` trait. This is where our actual endpoint handlers go.
impl NvServerApi for RestApi {
    type Context = ApiCtx;

    async fn example_endpoint(
        _ctx: RequestContext<Self::Context>,
    ) -> Result<HttpResponseOk<Stub>, HttpError> {
        Ok(HttpResponseOk(Stub {
            name: "example".to_string(),
        }))
    }
}

/// Shared app context, available to every endpoint handler.
pub struct ApiCtx {
    #[allow(unused)]
    /// Handle to the database.
    pub db: DatabaseConnection,
}

impl ApiCtx {
    /// Try to initialize the application context.
    async fn init(config: &Config) -> Result<Self, FatalError> {
        let db = crate::db::connection(config).await?;
        Ok(ApiCtx { db })
    }
}
