//! Handles interactions with the database.

pub mod entities;

use crate::config::Config;
use migration::{Migrator, MigratorTrait as _};
use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use secrecy::ExposeSecret as _;
use std::time::Duration;

/// Build database connection options using the given configuration.
pub fn connect_options(config: &Config) -> ConnectOptions {
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

    opt
}

/// Connect to the database using the given configuration without running migrations.
pub async fn connection_without_migrations(
    config: &Config,
) -> Result<DatabaseConnection, DatabaseConnectionError> {
    Database::connect(connect_options(config))
        .await
        .map_err(DatabaseConnectionError::Connect)
}

/// Connect to the database using the given configuration.
pub async fn connection(config: &Config) -> Result<DatabaseConnection, DatabaseConnectionError> {
    let db = connection_without_migrations(config).await?;

    run_all_migrations(&db).await?;

    Ok(db)
}

pub async fn run_all_migrations(db: &DatabaseConnection) -> Result<(), DatabaseConnectionError> {
    Migrator::up(db, None)
        .await
        .map_err(DatabaseConnectionError::Migrate)?;
    Ok(())
}

/// Failure to connect to or prepare the database.
#[derive(Debug)]
pub enum DatabaseConnectionError {
    Connect(sea_orm::DbErr),
    Migrate(sea_orm::DbErr),
}

impl std::fmt::Display for DatabaseConnectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Connect(_) => write!(f, "failed to connect to database"),
            Self::Migrate(_) => write!(f, "failed to run database migrations"),
        }
    }
}

impl std::error::Error for DatabaseConnectionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Connect(err) | Self::Migrate(err) => Some(err),
        }
    }
}
