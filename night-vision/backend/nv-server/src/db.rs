//! Handles interactions with the database.

use crate::{config::Config, error::FatalError};
use migration::{Migrator, MigratorTrait as _};
use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use secrecy::ExposeSecret as _;
use std::time::Duration;

/// Connect to the database using the given configuration.
pub async fn connection(config: &Config) -> Result<DatabaseConnection, FatalError> {
    let database_connection = config.database_connection();
    let mut opt = ConnectOptions::new(database_connection.expose_secret());

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

    run_all_migrations(&db).await?;

    Ok(db)
}

pub async fn run_all_migrations(db: &DatabaseConnection) -> Result<(), FatalError> {
    Migrator::up(db, None)
        .await
        .map_err(FatalError::FailedToConnectToDatabase)?;
    Ok(())
}
