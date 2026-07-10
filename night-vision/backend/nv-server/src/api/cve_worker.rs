//! Background CVE List sync worker.

use crate::error::FatalError;
use nv_common::{
    config::CveListWorkerConfig,
    cve::{
        git::{CommitSha, GitCliCveListGit},
        progress::NoopCveListSyncProgress,
        storage::has_cve_list_records,
        sync::{
            CveListSyncError, CveListSyncSummary,
            sync_cve_list_once_exclusive_with_progress_pipeline_config_and_timeout,
        },
    },
};
use sea_orm::DatabaseConnection;
use slog::{error, info, warn};
use tokio::task::JoinHandle;

/// Run the startup sync before the server begins accepting requests.
pub async fn sync_cve_list_on_startup(
    db: &DatabaseConnection,
    config: &CveListWorkerConfig,
    log: &slog::Logger,
) -> Result<(), FatalError> {
    match sync_cve_list(db, config).await {
        Ok(summary) => {
            log_startup_cve_list_sync_summary(log, &summary);
        }
        Err(error) => {
            handle_startup_cve_list_sync_error(db, log, error).await?;
        }
    }

    Ok(())
}

/// Spawn the recurring CVE List sync worker.
pub fn spawn_cve_list_worker(
    db: DatabaseConnection,
    config: CveListWorkerConfig,
    log: slog::Logger,
) -> JoinHandle<()> {
    tokio::spawn(run_cve_list_worker(db, config, log))
}

async fn run_cve_list_worker(
    db: DatabaseConnection,
    config: CveListWorkerConfig,
    log: slog::Logger,
) {
    info!(
        log,
        "started CVE List sync worker";
        "sync_interval_ms" => config.sync_interval().as_millis().to_string(),
        "sync_timeout_ms" => config.sync_timeout().as_millis().to_string(),
        "first_sync_timeout_ms" => config.first_sync_timeout().as_millis().to_string(),
    );

    loop {
        tokio::time::sleep(config.sync_interval()).await;

        match sync_cve_list(&db, &config).await {
            Ok(summary) => {
                log_scheduled_cve_list_sync_summary(&log, &summary);
            }
            Err(error) if error.is_sync_already_running() => {
                log_skipped_cve_list_sync(&log, "scheduled", &error);
            }
            Err(error) => {
                log_cve_list_sync_error(&log, &error);
            }
        }
    }
}

async fn sync_cve_list(
    db: &DatabaseConnection,
    config: &CveListWorkerConfig,
) -> Result<CveListSyncSummary, CveListSyncError> {
    let git = GitCliCveListGit::new(
        config.checkout_path().to_owned(),
        config.repository_url().clone(),
        config.repository_ref().clone(),
    );

    sync_cve_list_once_exclusive_with_progress_pipeline_config_and_timeout(
        db,
        &git,
        config.repository_url(),
        config.repository_ref(),
        &NoopCveListSyncProgress,
        config.sync_pipeline_config(),
        config.sync_timeout_config(),
    )
    .await
}

async fn startup_can_use_stored_cve_list_records(
    db: &DatabaseConnection,
    log: &slog::Logger,
    sync_error: &CveListSyncError,
) -> bool {
    match has_cve_list_records(db).await {
        Ok(has_records) => has_records,
        Err(lookup_error) => {
            error!(
                log,
                "failed to check stored CVE List records after startup sync failure";
                "sync_error" => sync_error.to_string(),
                "lookup_error" => lookup_error.to_string(),
            );
            false
        }
    }
}

async fn handle_startup_cve_list_sync_error(
    db: &DatabaseConnection,
    log: &slog::Logger,
    error: CveListSyncError,
) -> Result<(), FatalError> {
    if startup_can_use_stored_cve_list_records(db, log, &error).await {
        if error.is_sync_already_running() {
            log_skipped_cve_list_sync(log, "startup", &error);
        } else {
            log_startup_cve_list_sync_failure_permitted(log, &error);
        }

        Ok(())
    } else {
        Err(error.into())
    }
}

fn log_startup_cve_list_sync_summary(log: &slog::Logger, summary: &CveListSyncSummary) {
    let commit_sha = summary_commit_sha(summary);

    info!(
        log,
        "completed startup CVE List sync";
        "generation" => summary.generation,
        "status" => summary.status.to_string(),
        "commit_sha" => commit_sha,
        "records_seen" => summary.records_seen,
        "records_inserted" => summary.records_inserted,
        "records_updated" => summary.records_updated,
    );
}

fn log_startup_cve_list_sync_failure_permitted(log: &slog::Logger, error: &CveListSyncError) {
    warn!(
        log,
        "continuing startup after CVE List sync failure because stored records exist";
        "error" => error.to_string(),
    );
}

fn log_scheduled_cve_list_sync_summary(log: &slog::Logger, summary: &CveListSyncSummary) {
    let commit_sha = summary_commit_sha(summary);

    info!(
        log,
        "completed scheduled CVE List sync";
        "generation" => summary.generation,
        "status" => summary.status.to_string(),
        "commit_sha" => commit_sha,
        "records_seen" => summary.records_seen,
        "records_inserted" => summary.records_inserted,
        "records_updated" => summary.records_updated,
    );
}

fn log_skipped_cve_list_sync(log: &slog::Logger, sync_kind: &str, error: &CveListSyncError) {
    info!(
        log,
        "skipped CVE List sync because another sync is still running";
        "sync_kind" => sync_kind,
        "error" => error.to_string(),
    );
}

fn log_cve_list_sync_error(log: &slog::Logger, error: &CveListSyncError) {
    error!(
        log,
        "failed scheduled CVE List sync";
        "error" => error.to_string(),
    );
}

fn summary_commit_sha(summary: &CveListSyncSummary) -> &str {
    summary
        .commit_sha
        .as_ref()
        .map_or("<none>", CommitSha::as_str)
}

#[cfg(test)]
mod tests {
    use super::{
        CveListSyncError, handle_startup_cve_list_sync_error,
        startup_can_use_stored_cve_list_records,
    };
    use camino::Utf8PathBuf;
    use nv_common::{config::Config, cve::storage::CveListSyncRunError};
    use sea_orm::{DbBackend, MockDatabase, Value};
    use std::{collections::BTreeMap, time::Duration};

    #[test]
    fn worker_config_copies_cve_list_settings() {
        let mut config_path = Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        config_path.pop();
        config_path.push("nv-server.spookey");
        let config = Config::parse(&config_path).expect("default backend config should parse");
        let worker_config = config.cve_list_worker_config();

        assert_eq!(
            worker_config.repository_url(),
            &config.cve_list_repository_url
        );
        assert_eq!(
            worker_config.repository_ref(),
            &config.cve_list_repository_ref
        );
        assert_eq!(
            worker_config.sync_interval(),
            Duration::from_millis(config.cve_list_sync_interval)
        );
        assert_eq!(
            worker_config.sync_timeout(),
            Duration::from_millis(config.cve_list_sync_timeout)
        );
        assert_eq!(
            worker_config.first_sync_timeout(),
            Duration::from_millis(config.cve_list_first_sync_timeout)
        );
        assert_eq!(worker_config.checkout_path(), config.cve_list_checkout_path);
        assert_eq!(
            worker_config.parse_concurrency(),
            config.cve_list_parse_concurrency
        );
        assert_eq!(
            worker_config.write_batch_size(),
            config.cve_list_write_batch_size
        );
        assert_eq!(
            worker_config.write_channel_size(),
            config.cve_list_write_channel_size
        );
    }

    #[test]
    fn startup_can_use_stored_cve_list_records_when_records_exist() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![mock_cve_id_row("CVE-2026-1000")]])
            .into_connection();
        let log = slog::Logger::root(slog::Discard, slog::o!());
        let error = CveListSyncError::TimedOut(Duration::from_secs(1));

        let can_start = run_async(startup_can_use_stored_cve_list_records(&db, &log, &error));

        assert!(can_start);
    }

    #[test]
    fn startup_cannot_use_stored_cve_list_records_when_storage_is_empty() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([Vec::<BTreeMap<String, Value>>::new()])
            .into_connection();
        let log = slog::Logger::root(slog::Discard, slog::o!());
        let error = CveListSyncError::TimedOut(Duration::from_secs(1));

        let can_start = run_async(startup_can_use_stored_cve_list_records(&db, &log, &error));

        assert!(!can_start);
    }

    #[test]
    fn startup_can_skip_already_running_sync_when_records_exist() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![mock_cve_id_row("CVE-2026-1000")]])
            .into_connection();
        let log = slog::Logger::root(slog::Discard, slog::o!());
        let error = sync_already_running_error();

        let result = run_async(handle_startup_cve_list_sync_error(&db, &log, error));

        assert!(result.is_ok());
    }

    #[test]
    fn startup_fails_already_running_sync_when_storage_is_empty() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([Vec::<BTreeMap<String, Value>>::new()])
            .into_connection();
        let log = slog::Logger::root(slog::Discard, slog::o!());
        let error = sync_already_running_error();

        let result = run_async(handle_startup_cve_list_sync_error(&db, &log, error));

        assert!(result.is_err());
    }

    fn sync_already_running_error() -> CveListSyncError {
        CveListSyncError::StartSyncRun(CveListSyncRunError::SyncAlreadyRunning)
    }

    fn mock_cve_id_row(cve_id: &str) -> BTreeMap<String, Value> {
        BTreeMap::from([("cve_id".to_owned(), cve_id.to_owned().into())])
    }

    fn run_async<T>(future: impl std::future::Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime should build")
            .block_on(future)
    }
}
