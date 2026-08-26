//! Background CVE List sync worker.

use nv_common::{
    config::CveListWorkerConfig,
    cve::{
        git::{CommitSha, GitCliCveListGit},
        progress::NoopCveListSyncProgress,
        sync::{
            CveListSyncError, CveListSyncSummary,
            sync_cve_list_once_exclusive_with_progress_pipeline_config_and_timeout,
        },
    },
    error::ErrorSourceIterator as _,
};
use sea_orm::DatabaseConnection;
use slog::{error, info};
use tokio::task::JoinHandle;

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
    log_cve_list_pipeline_config(&log, &config, "worker");

    info!(
        log,
        "started CVE List sync worker";
        "sync_interval_ms" => config.sync_interval().as_millis().to_string(),
        "sync_timeout_ms" => config.sync_timeout().as_millis().to_string(),
        "first_sync_timeout_ms" => config.first_sync_timeout().as_millis().to_string(),
    );

    loop {
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

        tokio::time::sleep(config.sync_interval()).await;
    }
}

async fn sync_cve_list(
    db: &DatabaseConnection,
    config: &CveListWorkerConfig,
) -> Result<CveListSyncSummary, CveListSyncError> {
    let git = GitCliCveListGit::new_with_record_max_bytes(
        config.checkout_path().to_owned(),
        config.repository_url().clone(),
        config.repository_ref().clone(),
        config.record_max_bytes(),
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
        "error" => format_cve_list_sync_error(error),
    );
}

fn format_cve_list_sync_error(error: &CveListSyncError) -> String {
    error
        .sources_iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(": ")
}

fn log_cve_list_pipeline_config(
    log: &slog::Logger,
    config: &CveListWorkerConfig,
    sync_context: &str,
) {
    if !config.parse_concurrency_source().is_computed_default()
        && !config.write_batch_size_source().is_computed_default()
        && !config.write_channel_size_source().is_computed_default()
    {
        return;
    }

    info!(
        log,
        "resolved CVE List sync pipeline defaults";
        "sync_context" => sync_context,
        "parse_concurrency" => config.parse_concurrency(),
        "parse_concurrency_source" => config.parse_concurrency_source().to_string(),
        "write_batch_size" => config.write_batch_size(),
        "write_batch_size_source" => config.write_batch_size_source().to_string(),
        "write_channel_size" => config.write_channel_size(),
        "write_channel_size_source" => config.write_channel_size_source().to_string(),
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
    use super::{CveListSyncError, format_cve_list_sync_error};
    use camino::{Utf8Path, Utf8PathBuf};
    use nv_common::config::Config;
    use std::{
        fs,
        sync::atomic::{AtomicUsize, Ordering},
        time::Duration,
    };

    static TEST_FILE_COUNTER: AtomicUsize = AtomicUsize::new(0);

    struct TempConfigFile {
        path: Utf8PathBuf,
    }

    impl TempConfigFile {
        fn default_backend_config_with_inline_test_database() -> Self {
            let mut default_config_path = Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            default_config_path.pop();
            default_config_path.push("nv-server.spookey");

            let config = fs::read_to_string(&default_config_path)
                .expect("default backend config should be readable");
            let database_connection_file =
                "database-connection-file = \"../.secrets/local-development-database-url\"";
            assert!(
                config.contains(database_connection_file),
                "default backend config should use the local development database URL file"
            );
            let config = config.replace(
                database_connection_file,
                "database-connection = \"postgres://localhost:5432/nv\"",
            );

            let id = TEST_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "nv-server-cve-worker-config-test-{}-{id}.spookey",
                std::process::id()
            ));
            fs::write(&path, config).expect("failed to write test config file");

            Self {
                path: Utf8PathBuf::from_path_buf(path)
                    .expect("test temp path should be valid UTF-8"),
            }
        }

        fn path(&self) -> &Utf8Path {
            &self.path
        }
    }

    impl Drop for TempConfigFile {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.path);
        }
    }

    #[test]
    fn worker_config_copies_cve_list_settings() {
        let config_file = TempConfigFile::default_backend_config_with_inline_test_database();
        let config = Config::parse(config_file.path())
            .expect("default backend config with a test database should parse");
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
            worker_config.record_max_bytes(),
            config.cve_record_max_bytes
        );
        assert_eq!(
            worker_config.write_channel_size(),
            config.cve_list_write_channel_size
        );
        assert_eq!(
            worker_config.parse_concurrency_source(),
            config.cve_list_parse_concurrency_source
        );
        assert_eq!(
            worker_config.write_batch_size_source(),
            config.cve_list_write_batch_size_source
        );
        assert_eq!(
            worker_config.write_channel_size_source(),
            config.cve_list_write_channel_size_source
        );
    }

    #[test]
    fn cve_list_sync_error_format_includes_error_context() {
        let error =
            CveListSyncError::Git(nv_common::cve::git::CveListGitError::InvalidCheckoutDir(
                Utf8PathBuf::from("target/cve-list/cvelistV5"),
            ));

        assert_eq!(
            format_cve_list_sync_error(&error),
            "failed to access CVE List Git repository: \
             CVE List checkout path exists but is not a Git repository: \
             target/cve-list/cvelistV5"
        );
    }
}
