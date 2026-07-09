//! Background CVE List sync worker.

use crate::error::FatalError;
use nv_common::{
    config::CveListWorkerConfig,
    cve::{
        git::{CommitSha, GitCliCveListGit},
        progress::NoopCveListSyncProgress,
        sync::{
            CveListSyncError, CveListSyncSummary,
            sync_cve_list_once_with_progress_and_pipeline_config,
        },
    },
};
use sea_orm::DatabaseConnection;
use slog::{error, info};
use tokio::task::JoinHandle;

/// Run the startup sync before the server begins accepting requests.
pub async fn sync_cve_list_on_startup(
    db: &DatabaseConnection,
    config: &CveListWorkerConfig,
    log: &slog::Logger,
) -> Result<(), FatalError> {
    let summary = sync_cve_list(db, config).await?;
    log_startup_cve_list_sync_summary(log, &summary);

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
    );

    loop {
        tokio::time::sleep(config.sync_interval()).await;

        match sync_cve_list(&db, &config).await {
            Ok(summary) => {
                log_scheduled_cve_list_sync_summary(&log, &summary);
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

    sync_cve_list_once_with_progress_and_pipeline_config(
        db,
        &git,
        config.repository_url(),
        config.repository_ref(),
        &NoopCveListSyncProgress,
        config.sync_pipeline_config(),
    )
    .await
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
    use camino::Utf8PathBuf;
    use nv_common::config::Config;
    use std::time::Duration;

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
}
