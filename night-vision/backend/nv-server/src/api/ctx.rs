use crate::{config::Config, error::FatalError};
use sea_orm::DatabaseConnection;

/// Shared app context, available to every endpoint handler.
///
/// In a request handler, get it with `ctx.context()`.
pub struct ApiCtx {
    /// Handle to the database.
    db: DatabaseConnection,
}

impl ApiCtx {
    /// Try to initialize the application context.
    pub async fn init(config: &Config) -> Result<Self, FatalError> {
        let db = crate::db::connection(config).await?;
        Ok(ApiCtx { db })
    }

    /// Get a handle to the database.
    pub fn db(&self) -> &DatabaseConnection {
        &self.db
    }
}
