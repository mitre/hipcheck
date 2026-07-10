use anyhow::{Context as _, Result, anyhow, bail};
use camino::Utf8Path;
use chrono::Utc;
use migration::{Migrator, MigratorTrait as _};
use nv_common::{
    config::Config,
    cve::{
        git::{CveListGit as _, GitCliCveListGit},
        sync::CVE_LIST_SYNC_ADVISORY_LOCK_ID,
    },
    db::entities::{cve_list_sync_runs, cve_list_sync_runs::Model as CveListSyncRun},
    rt,
};
use sea_orm::{
    ColumnTrait as _, ConnectOptions, ConnectionTrait as _, Database, DatabaseBackend,
    DatabaseConnection, EntityTrait as _, QueryFilter as _, QueryOrder as _, QueryResult,
    QuerySelect as _, Statement, TransactionTrait as _,
};
use secrecy::ExposeSecret as _;
use std::{collections::HashSet, fs, time::Duration};

const REQUIRED_CVE_TABLES: &[&str] = &[
    "public.cve_list_records",
    "public.cve_list_record_staging",
    "public.cve_list_sync_runs",
];

pub fn command() -> clap::Command {
    clap::Command::new("doctor").about("Check CVE List ingest operational health")
}

pub fn run(config: &Config, _matches: &clap::ArgMatches) -> Result<()> {
    let runtime = rt::AsyncRuntime::new(config).context("failed to create async runtime")?;
    runtime.block_on(doctor(config))
}

async fn doctor(config: &Config) -> Result<()> {
    let worker_config = config.cve_list_worker_config();
    let mut ok = true;

    let db = match connect_without_migrations(config).await {
        Ok(db) => {
            println!("database_reachable: true");
            Some(db)
        }
        Err(error) => {
            println!("database_reachable: false");
            println!("database_error: {}", first_error_line(&error));
            ok = false;
            None
        }
    };

    if let Some(db) = db.as_ref() {
        let migration_table_present = check_table_present(db, "public.seaql_migrations")
            .await
            .context("failed to check migration table presence")?;
        println!("migration_table_present: {migration_table_present}");
        ok &= migration_table_present;

        if migration_table_present {
            let migrations_current = check_migrations_current(db)
                .await
                .context("failed to check CVE List migration status")?;
            println!(
                "migrations_applied: {}",
                migrations_current.applied_versions
            );
            println!(
                "migrations_required: {}",
                migrations_current.required_versions
            );
            println!(
                "migrations_pending: {}",
                format_pending_migrations(&migrations_current.pending_versions)
            );
            ok &= migrations_current.pending_versions.is_empty();
        } else {
            println!("migrations_applied: <unknown>");
            println!(
                "migrations_required: {}",
                required_migration_versions().len()
            );
            println!("migrations_pending: <unknown>");
        }

        let table_statuses = check_cve_tables_present(db)
            .await
            .context("failed to check CVE List table presence")?;
        for table in &table_statuses {
            println!("table_present_{}: {}", table.key(), table.present);
            ok &= table.present;
        }

        let all_cve_tables_present = table_statuses.iter().all(|table| table.present);
        if all_cve_tables_present {
            let sync_lock_available = try_acquire_sync_lock(db)
                .await
                .context("failed to check CVE List sync advisory lock")?;
            println!("sync_lock_available: {sync_lock_available}");
            ok &= sync_lock_available;

            let latest_run = latest_sync_run(db)
                .await
                .context("failed to read latest CVE List sync run")?;
            print_latest_run(latest_run.as_ref());

            let running_run_age = latest_running_sync_run_age_seconds(db)
                .await
                .context("failed to read running CVE List sync run age")?;
            print_running_run_age(running_run_age);
        } else {
            println!("sync_lock_available: <skipped>");
            println!("latest_run_generation: <skipped>");
            println!("latest_run_status: <skipped>");
            println!("latest_run_checked_at: <skipped>");
            println!("running_run_generation: <skipped>");
            println!("running_run_age_seconds: <skipped>");
            ok = false;
        }
    } else {
        println!("migration_table_present: <skipped>");
        println!("migrations_applied: <skipped>");
        println!(
            "migrations_required: {}",
            required_migration_versions().len()
        );
        println!("migrations_pending: <skipped>");
        for table in REQUIRED_CVE_TABLES {
            println!("table_present_{}: <skipped>", table_key(table));
        }
        println!("sync_lock_available: <skipped>");
        println!("latest_run_generation: <skipped>");
        println!("latest_run_status: <skipped>");
        println!("latest_run_checked_at: <skipped>");
        println!("running_run_generation: <skipped>");
        println!("running_run_age_seconds: <skipped>");
    }

    let checkout_status = checkout_path_status(worker_config.checkout_path());
    println!("checkout_path: {}", worker_config.checkout_path());
    println!("checkout_path_exists: {}", checkout_status.exists);
    println!("checkout_path_writable: {}", checkout_status.writable);
    ok &= checkout_status.exists && checkout_status.writable;

    if checkout_status.exists {
        let git = GitCliCveListGit::new(
            worker_config.checkout_path().to_owned(),
            worker_config.repository_url().clone(),
            worker_config.repository_ref().clone(),
        );
        match git
            .resolve_ref(worker_config.repository_ref().as_str())
            .await
        {
            Ok(commit) => {
                println!("configured_ref_resolvable: true");
                println!("configured_ref_commit: {}", commit.as_str());
            }
            Err(error) => {
                println!("configured_ref_resolvable: false");
                println!("configured_ref_error: {}", first_error_line(&error));
                ok = false;
            }
        }
    } else {
        println!("configured_ref_resolvable: <skipped>");
        println!("configured_ref_commit: <skipped>");
    }

    println!("doctor_ok: {ok}");

    if !ok {
        bail!("CVE List doctor found failing checks");
    }

    Ok(())
}

async fn connect_without_migrations(config: &Config) -> Result<DatabaseConnection> {
    let mut options = ConnectOptions::new(config.database_connection().expose_secret());

    if let Some(database_max_connections) = config.database_max_connections {
        options.max_connections(database_max_connections);
    }

    if let Some(database_min_connections) = config.database_min_connections {
        options.min_connections(database_min_connections);
    }

    if let Some(database_connect_timeout) = config.database_connect_timeout {
        options.connect_timeout(Duration::from_millis(database_connect_timeout));
    }

    if let Some(database_idle_timeout) = config.database_idle_timeout {
        options.idle_timeout(Duration::from_millis(database_idle_timeout));
    }

    if let Some(database_acquire_timeout) = config.database_acquire_timeout {
        options.acquire_timeout(Duration::from_millis(database_acquire_timeout));
    }

    if let Some(database_max_lifetime) = config.database_max_lifetime {
        options.max_lifetime(Duration::from_millis(database_max_lifetime));
    }

    Database::connect(options)
        .await
        .context("failed to connect to database")
}

async fn check_table_present(db: &DatabaseConnection, table: &str) -> Result<bool> {
    let statement = Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "SELECT to_regclass($1) IS NOT NULL",
        [table.into()],
    );
    let result = db
        .query_one_raw(statement)
        .await?
        .ok_or_else(|| anyhow!("table-presence query returned no row"))?;

    bool_from_first_column(&result)
}

async fn check_cve_tables_present(db: &DatabaseConnection) -> Result<Vec<TableStatus>> {
    let mut statuses = Vec::with_capacity(REQUIRED_CVE_TABLES.len());

    for table in REQUIRED_CVE_TABLES {
        statuses.push(TableStatus {
            table,
            present: check_table_present(db, table).await?,
        });
    }

    Ok(statuses)
}

async fn check_migrations_current(db: &DatabaseConnection) -> Result<MigrationStatus> {
    let required_versions = required_migration_versions();
    let required_version_set = required_versions.iter().cloned().collect::<HashSet<_>>();
    let statement = Statement::from_sql_and_values(
        DatabaseBackend::Postgres,
        "SELECT version FROM public.seaql_migrations ORDER BY version",
        [],
    );
    let rows = db.query_all_raw(statement).await?;
    let applied_versions = rows
        .iter()
        .map(|row| row.try_get_by_index::<String>(0))
        .collect::<Result<HashSet<_>, _>>()?;
    let mut pending_versions = required_versions
        .iter()
        .filter(|version| !applied_versions.contains(*version))
        .cloned()
        .collect::<Vec<_>>();
    pending_versions.sort();

    Ok(MigrationStatus {
        applied_versions: applied_versions.intersection(&required_version_set).count(),
        required_versions: required_versions.len(),
        pending_versions,
    })
}

fn required_migration_versions() -> Vec<String> {
    Migrator::migrations()
        .into_iter()
        .map(|migration| migration.name().to_owned())
        .collect()
}

async fn try_acquire_sync_lock(db: &DatabaseConnection) -> Result<bool> {
    let transaction = db.begin().await?;
    let statement = Statement::from_string(
        DatabaseBackend::Postgres,
        format!("SELECT pg_try_advisory_xact_lock({CVE_LIST_SYNC_ADVISORY_LOCK_ID})"),
    );
    let result = transaction
        .query_one_raw(statement)
        .await?
        .ok_or_else(|| anyhow!("CVE List sync advisory-lock query returned no row"))?;
    let acquired = bool_from_first_column(&result)?;

    transaction.rollback().await?;

    Ok(acquired)
}

async fn latest_sync_run(db: &DatabaseConnection) -> Result<Option<CveListSyncRun>> {
    cve_list_sync_runs::Entity::find()
        .order_by_desc(cve_list_sync_runs::Column::Generation)
        .limit(1)
        .one(db)
        .await
        .map_err(Into::into)
}

async fn latest_running_sync_run_age_seconds(
    db: &DatabaseConnection,
) -> Result<Option<RunningRunAge>> {
    let run = cve_list_sync_runs::Entity::find()
        .filter(cve_list_sync_runs::Column::Status.eq("running"))
        .order_by_desc(cve_list_sync_runs::Column::Generation)
        .limit(1)
        .one(db)
        .await?;

    Ok(run.map(|run| RunningRunAge {
        generation: run.generation,
        age_seconds: Utc::now()
            .signed_duration_since(run.checked_at)
            .num_seconds()
            .max(0),
    }))
}

fn checkout_path_status(path: &Utf8Path) -> CheckoutPathStatus {
    match fs::metadata(path) {
        Ok(metadata) => CheckoutPathStatus {
            exists: metadata.is_dir(),
            writable: metadata.is_dir() && !metadata.permissions().readonly(),
        },
        Err(_) => CheckoutPathStatus {
            exists: false,
            writable: false,
        },
    }
}

fn print_latest_run(latest_run: Option<&CveListSyncRun>) {
    if let Some(latest_run) = latest_run {
        println!("latest_run_generation: {}", latest_run.generation);
        println!("latest_run_status: {}", latest_run.status);
        println!("latest_run_checked_at: {}", latest_run.checked_at);
    } else {
        println!("latest_run_generation: <none>");
        println!("latest_run_status: <none>");
        println!("latest_run_checked_at: <none>");
    }
}

fn print_running_run_age(running_run_age: Option<RunningRunAge>) {
    if let Some(running_run_age) = running_run_age {
        println!("running_run_generation: {}", running_run_age.generation);
        println!("running_run_age_seconds: {}", running_run_age.age_seconds);
    } else {
        println!("running_run_generation: <none>");
        println!("running_run_age_seconds: <none>");
    }
}

fn bool_from_first_column(result: &QueryResult) -> Result<bool> {
    result
        .try_get_by_index(0)
        .context("failed to read boolean query result")
}

fn format_pending_migrations(pending_versions: &[String]) -> String {
    if pending_versions.is_empty() {
        "<none>".to_owned()
    } else {
        pending_versions.join(",")
    }
}

fn first_error_line(error: &impl std::fmt::Display) -> String {
    error.to_string().lines().next().unwrap_or("").to_owned()
}

fn table_key(table: &str) -> String {
    table.replace('.', "_")
}

struct TableStatus {
    table: &'static str,
    present: bool,
}

impl TableStatus {
    fn key(&self) -> String {
        table_key(self.table)
    }
}

struct MigrationStatus {
    applied_versions: usize,
    required_versions: usize,
    pending_versions: Vec<String>,
}

struct RunningRunAge {
    generation: i64,
    age_seconds: i64,
}

struct CheckoutPathStatus {
    exists: bool,
    writable: bool,
}

#[cfg(test)]
mod tests {
    use super::{command, format_pending_migrations, table_key};

    #[test]
    fn cve_doctor_accepts_no_arguments() {
        command()
            .try_get_matches_from(["doctor"])
            .expect("doctor should parse");
    }

    #[test]
    fn pending_migrations_print_none_when_empty() {
        assert_eq!(format_pending_migrations(&[]), "<none>");
    }

    #[test]
    fn pending_migrations_print_comma_separated_versions() {
        assert_eq!(
            format_pending_migrations(&["a".to_owned(), "b".to_owned()]),
            "a,b"
        );
    }

    #[test]
    fn table_keys_replace_schema_separator() {
        assert_eq!(
            table_key("public.cve_list_records"),
            "public_cve_list_records"
        );
    }
}
