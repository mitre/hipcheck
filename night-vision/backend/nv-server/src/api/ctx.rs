use crate::error::FatalError;
use nv_common::{config::Config, npm::elaboration::ElaborationLimits};
use sea_orm::DatabaseConnection;
use url::Url;

/// Shared app context, available to every endpoint handler.
///
/// In a request handler, get it with `ctx.context()`.
pub struct ApiCtx {
    /// Handle to the database.
    db: DatabaseConnection,
    npm_registry_url: Url,
    package_elaboration_limits: ElaborationLimits,
    package_elaboration_max_packument_bytes: usize,
}

impl ApiCtx {
    /// Try to initialize the application context.
    pub async fn init(config: &Config) -> Result<Self, FatalError> {
        let db = nv_common::db::connection(config).await?;
        Ok(Self {
            db,
            npm_registry_url: config.npm_registry_url.clone(),
            package_elaboration_limits: config.package_elaboration_limits(),
            package_elaboration_max_packument_bytes: config.package_elaboration_max_packument_bytes,
        })
    }

    /// Get a handle to the database.
    pub fn db(&self) -> &DatabaseConnection {
        &self.db
    }

    pub fn npm_registry_url(&self) -> &Url {
        &self.npm_registry_url
    }
    pub fn package_elaboration_limits(&self) -> ElaborationLimits {
        self.package_elaboration_limits.clone()
    }
    pub fn package_elaboration_max_packument_bytes(&self) -> usize {
        self.package_elaboration_max_packument_bytes
    }
}
