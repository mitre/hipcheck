use crate::{config::Config, error::FatalError};
use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use std::time::Duration;

/// Connect to the database using the given configuration.
pub async fn connection(config: &Config) -> Result<DatabaseConnection, FatalError> {
    let mut opt = ConnectOptions::new(&config.database_connection);

    if let Some(database_max_connections) = config.database_max_connections {
        opt.max_connections(database_max_connections);
    }

    if let Some(database_min_connections) = config.database_min_connections {
        opt.min_connections(database_min_connections);
    }

    if let Some(database_connect_timeout) = config.database_connect_timeout {
        opt.connect_timeout(Duration::from_millis(database_connect_timeout));
    }

    if let Some(database_idle_timeout) = config.database_idle_timeout {
        opt.idle_timeout(Duration::from_millis(database_idle_timeout));
    }

    if let Some(database_acquire_timeout) = config.database_acquire_timeout {
        opt.acquire_timeout(Duration::from_millis(database_acquire_timeout));
    }

    if let Some(database_max_lifetime) = config.database_max_lifetime {
        opt.max_lifetime(Duration::from_millis(database_max_lifetime));
    }

    let db = Database::connect(opt)
        .await
        .map_err(FatalError::FailedToConnectToDatabase)?;

    Ok(db)
}
