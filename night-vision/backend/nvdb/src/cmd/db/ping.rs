use anyhow::{Context as _, Result, anyhow};
use nv_common::{config::Config, db, rt};
use sea_orm::{ConnectionTrait as _, Statement};

pub fn command() -> clap::Command {
    clap::Command::new("ping").about("Check whether the configured database is reachable")
}

pub fn run(config: &Config) -> Result<()> {
    let runtime = rt::AsyncRuntime::new(config).context("failed to create async runtime")?;

    runtime
        .block_on(ping_database(config))
        .map_err(redact_ping_error)
}

async fn ping_database(config: &Config) -> Result<()> {
    let db = db::connection_without_migrations(config)
        .await
        .context("failed to connect to database")?;

    let statement = Statement::from_string(db.get_database_backend(), "SELECT 1".to_owned());
    db.query_one_raw(statement)
        .await
        .context("database ping query failed")?
        .ok_or_else(|| anyhow!("database ping query returned no row"))?;

    println!("Database reachable.");

    Ok(())
}

fn redact_ping_error(_error: anyhow::Error) -> anyhow::Error {
    DatabasePingError.into()
}

#[derive(Debug)]
struct DatabasePingError;

impl std::fmt::Display for DatabasePingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Database ping failed: unable to reach the configured database."
        )
    }
}

impl std::error::Error for DatabasePingError {}

#[cfg(test)]
mod tests {
    use super::{DatabasePingError, redact_ping_error};
    use anyhow::{Result, anyhow};

    #[test]
    fn success_result_is_unchanged() {
        let result: Result<()> = Ok(());

        result.map_err(redact_ping_error).unwrap();
    }

    #[test]
    fn database_ping_error_message_is_concise_and_safe() {
        let message = DatabasePingError.to_string();

        assert!(message.contains("Database ping failed"));
        assert!(!message.contains("password"));
        assert!(!message.contains("postgres://"));
    }

    #[test]
    fn bad_configuration_errors_are_redacted() {
        let error = anyhow!("invalid database URL: postgres://user:password@localhost:5432/nv");

        let result: Result<()> = Err(error);
        let redacted = result
            .map_err(redact_ping_error)
            .expect_err("invalid database URL should fail");

        let message = redacted.to_string();

        assert!(message.contains("Database ping failed"));
        assert!(!message.contains("password"));
        assert!(!message.contains("postgres://"));
    }

    #[test]
    fn connection_failures_are_redacted() {
        let error = anyhow!(
            "failed to connect to database: password authentication failed for user 'nvdb'"
        );

        let result: Result<()> = Err(error);
        let redacted = result
            .map_err(redact_ping_error)
            .expect_err("connection failure should fail");

        let message = redacted.to_string();

        assert!(message.contains("Database ping failed"));
        assert!(!message.contains("password authentication failed"));
        assert!(!message.contains("nvdb"));
    }
}
