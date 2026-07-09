//! Composed CVE List sync routine.

use crate::cve::{
    git::{CommitSha, CveListGit, CveListGitError, GitRef},
    progress::{CveListSyncProgress, CveListSyncProgressReporter, NoopCveListSyncProgress},
    repository::{
        CveListRepositoryError, DEFAULT_CVE_RECORD_PARSE_CONCURRENCY,
        list_all_cve_files_with_progress, list_changed_cve_files_with_progress, send_cve_files,
    },
    storage::{
        CveListSyncRunError, CveListSyncRunStatus, DEFAULT_CVE_LIST_RECORD_WRITE_CHANNEL_SIZE,
        FinishCveListSyncRun, WriteCveListRecordsError, finish_cve_list_sync_run,
        insert_new_cve_list_records_from_receiver_with_batch_size_and_progress,
        last_successful_cve_list_sync_commit, start_cve_list_sync_run,
        write_cve_list_records_from_receiver_with_batch_size_and_progress,
    },
};
use sea_orm::ConnectionTrait;
use tokio::sync::mpsc;
use url::Url;

/// Outcome of a CVE List sync attempt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CveListSyncSummary {
    /// Sync-run generation.
    pub generation: i64,
    /// Terminal sync status.
    pub status: CveListSyncRunStatus,
    /// Git commit synced by this run, when one was resolved.
    pub commit_sha: Option<CommitSha>,
    /// Number of parsed records seen by this run.
    pub records_seen: usize,
    /// Number of records inserted by this run.
    pub records_inserted: usize,
    /// Number of records updated by this run.
    pub records_updated: usize,
}

/// Pipeline tuning for a CVE List sync attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CveListSyncPipelineConfig {
    /// Maximum number of CVE List records to write in one database batch.
    pub write_batch_size: usize,
    /// Maximum number of CVE List records to parse concurrently.
    pub parse_concurrency: usize,
    /// Number of parsed CVE List records to buffer before database writes.
    pub write_channel_size: usize,
}

impl CveListSyncPipelineConfig {
    fn with_write_batch_size(write_batch_size: usize) -> Self {
        Self {
            write_batch_size,
            parse_concurrency: DEFAULT_CVE_RECORD_PARSE_CONCURRENCY,
            write_channel_size: DEFAULT_CVE_LIST_RECORD_WRITE_CHANNEL_SIZE,
        }
    }
}

/// Sync CVE List records from Git into database storage once.
pub async fn sync_cve_list_once<C, G>(
    db: &C,
    git: &G,
    repository_url: &Url,
    repository_ref: &GitRef,
    write_batch_size: usize,
) -> Result<CveListSyncSummary, CveListSyncError>
where
    C: ConnectionTrait,
    G: CveListGit + Sync,
{
    let pipeline_config = CveListSyncPipelineConfig::with_write_batch_size(write_batch_size);

    sync_cve_list_once_with_progress_and_pipeline_config(
        db,
        git,
        repository_url,
        repository_ref,
        &NoopCveListSyncProgress,
        pipeline_config,
    )
    .await
}

/// Sync CVE List records from Git into database storage once.
pub async fn sync_cve_list_once_with_parse_concurrency<C, G>(
    db: &C,
    git: &G,
    repository_url: &Url,
    repository_ref: &GitRef,
    write_batch_size: usize,
    parse_concurrency: usize,
) -> Result<CveListSyncSummary, CveListSyncError>
where
    C: ConnectionTrait,
    G: CveListGit + Sync,
{
    let pipeline_config = CveListSyncPipelineConfig {
        write_batch_size,
        parse_concurrency,
        write_channel_size: DEFAULT_CVE_LIST_RECORD_WRITE_CHANNEL_SIZE,
    };

    sync_cve_list_once_with_progress_and_pipeline_config(
        db,
        git,
        repository_url,
        repository_ref,
        &NoopCveListSyncProgress,
        pipeline_config,
    )
    .await
}

/// Sync CVE List records from Git into database storage once.
pub async fn sync_cve_list_once_with_parse_and_write_channel_config<C, G>(
    db: &C,
    git: &G,
    repository_url: &Url,
    repository_ref: &GitRef,
    write_batch_size: usize,
    parse_concurrency: usize,
    write_channel_size: usize,
) -> Result<CveListSyncSummary, CveListSyncError>
where
    C: ConnectionTrait,
    G: CveListGit + Sync,
{
    let pipeline_config = CveListSyncPipelineConfig {
        write_batch_size,
        parse_concurrency,
        write_channel_size,
    };

    sync_cve_list_once_with_progress_and_pipeline_config(
        db,
        git,
        repository_url,
        repository_ref,
        &NoopCveListSyncProgress,
        pipeline_config,
    )
    .await
}

/// Sync CVE List records from Git into database storage once, reporting progress.
pub async fn sync_cve_list_once_with_progress<C, G, P>(
    db: &C,
    git: &G,
    repository_url: &Url,
    repository_ref: &GitRef,
    write_batch_size: usize,
    progress: &P,
) -> Result<CveListSyncSummary, CveListSyncError>
where
    C: ConnectionTrait,
    G: CveListGit + Sync,
    P: CveListSyncProgressReporter + ?Sized,
{
    let pipeline_config = CveListSyncPipelineConfig::with_write_batch_size(write_batch_size);

    sync_cve_list_once_with_progress_and_pipeline_config(
        db,
        git,
        repository_url,
        repository_ref,
        progress,
        pipeline_config,
    )
    .await
}

/// Sync CVE List records from Git into database storage once, reporting progress.
pub async fn sync_cve_list_once_with_progress_and_parse_concurrency<C, G, P>(
    db: &C,
    git: &G,
    repository_url: &Url,
    repository_ref: &GitRef,
    progress: &P,
    pipeline_config: CveListSyncPipelineConfig,
) -> Result<CveListSyncSummary, CveListSyncError>
where
    C: ConnectionTrait,
    G: CveListGit + Sync,
    P: CveListSyncProgressReporter + ?Sized,
{
    sync_cve_list_once_with_progress_and_pipeline_config(
        db,
        git,
        repository_url,
        repository_ref,
        progress,
        pipeline_config,
    )
    .await
}

/// Sync CVE List records from Git into database storage once, reporting progress.
pub async fn sync_cve_list_once_with_progress_and_pipeline_config<C, G, P>(
    db: &C,
    git: &G,
    repository_url: &Url,
    repository_ref: &GitRef,
    progress: &P,
    pipeline_config: CveListSyncPipelineConfig,
) -> Result<CveListSyncSummary, CveListSyncError>
where
    C: ConnectionTrait,
    G: CveListGit + Sync,
    P: CveListSyncProgressReporter + ?Sized,
{
    let generation = start_cve_list_sync_run(db, repository_url, repository_ref)
        .await
        .map_err(CveListSyncError::StartSyncRun)?;
    progress.report(CveListSyncProgress::Started { generation });
    let mut resolved_commit = None;

    let sync_result =
        run_cve_list_sync(db, git, pipeline_config, &mut resolved_commit, progress).await;

    finish_cve_list_sync(db, generation, resolved_commit, sync_result, progress).await
}

async fn finish_cve_list_sync<C, P>(
    db: &C,
    generation: i64,
    resolved_commit: Option<CommitSha>,
    sync_result: Result<FinishCveListSyncRun, CveListSyncError>,
    progress: &P,
) -> Result<CveListSyncSummary, CveListSyncError>
where
    C: ConnectionTrait,
    P: CveListSyncProgressReporter + ?Sized,
{
    let completion = match sync_result {
        Ok(completion) => completion,
        Err(error) => {
            let error_message = error.to_string();
            let failed_completion = FinishCveListSyncRun {
                status: CveListSyncRunStatus::Failed,
                commit_sha: resolved_commit,
                records_seen: 0,
                records_inserted: 0,
                records_updated: 0,
                error: Some(error_message.clone()),
            };

            finish_cve_list_sync_run(db, generation, failed_completion)
                .await
                .map_err(|finish_error| CveListSyncError::FinishFailedSyncRun {
                    sync_error: error_message,
                    finish_error,
                })?;

            return Err(error);
        }
    };

    progress.report(CveListSyncProgress::FinishStarted);
    finish_cve_list_sync_run(db, generation, completion.clone())
        .await
        .map_err(CveListSyncError::FinishSyncRun)?;
    progress.report(CveListSyncProgress::Finished);

    Ok(CveListSyncSummary {
        generation,
        status: completion.status,
        commit_sha: completion.commit_sha,
        records_seen: completion.records_seen,
        records_inserted: completion.records_inserted,
        records_updated: completion.records_updated,
    })
}

async fn run_cve_list_sync<C, G, P>(
    db: &C,
    git: &G,
    pipeline_config: CveListSyncPipelineConfig,
    resolved_commit: &mut Option<CommitSha>,
    progress: &P,
) -> Result<FinishCveListSyncRun, CveListSyncError>
where
    C: ConnectionTrait,
    G: CveListGit + Sync,
    P: CveListSyncProgressReporter + ?Sized,
{
    progress.report(CveListSyncProgress::GitCheckoutStarted);
    git.ensure_checkout().await.map_err(CveListSyncError::Git)?;
    progress.report(CveListSyncProgress::GitFetchStarted);
    git.fetch().await.map_err(CveListSyncError::Git)?;

    progress.report(CveListSyncProgress::GitRefResolveStarted);
    let new_commit = git
        .resolve_ref("FETCH_HEAD")
        .await
        .map_err(CveListSyncError::Git)?;
    *resolved_commit = Some(new_commit.clone());

    progress.report(CveListSyncProgress::PreviousSyncLookupStarted);
    let last_successful_commit = last_successful_cve_list_sync_commit(db)
        .await
        .map_err(CveListSyncError::SyncRunMetadata)?;

    if last_successful_commit.as_ref() == Some(&new_commit) {
        progress.report(CveListSyncProgress::NotModified);
        return Ok(FinishCveListSyncRun {
            status: CveListSyncRunStatus::NotModified,
            commit_sha: Some(new_commit),
            records_seen: 0,
            records_inserted: 0,
            records_updated: 0,
            error: None,
        });
    }

    if pipeline_config.write_channel_size == 0 {
        return Err(CveListSyncError::Storage(
            WriteCveListRecordsError::InvalidWriteChannelSize,
        ));
    }

    let first_run = last_successful_commit.is_none();
    let paths = match last_successful_commit {
        Some(old_commit) => {
            list_changed_cve_files_with_progress(git, &old_commit, &new_commit, progress).await
        }
        None => list_all_cve_files_with_progress(git, &new_commit, progress).await,
    }
    .map_err(CveListSyncError::Repository)?;

    let total_records = paths.len();
    let (sender, receiver) = mpsc::channel(pipeline_config.write_channel_size);
    let parse_records = send_cve_files(
        git,
        &new_commit,
        paths,
        progress,
        pipeline_config.parse_concurrency,
        sender,
    );
    let write_records = async {
        if first_run {
            insert_new_cve_list_records_from_receiver_with_batch_size_and_progress(
                db,
                receiver,
                pipeline_config.write_batch_size,
                total_records,
                progress,
            )
            .await
        } else {
            write_cve_list_records_from_receiver_with_batch_size_and_progress(
                db,
                receiver,
                pipeline_config.write_batch_size,
                total_records,
                progress,
            )
            .await
        }
    };
    let (parse_result, write_result) = tokio::join!(parse_records, write_records);
    let write_summary = write_result.map_err(CveListSyncError::Storage)?;
    parse_result.map_err(CveListSyncError::Repository)?;

    Ok(FinishCveListSyncRun {
        status: CveListSyncRunStatus::Success,
        commit_sha: Some(new_commit),
        records_seen: write_summary.records_seen,
        records_inserted: write_summary.records_inserted,
        records_updated: write_summary.records_updated,
        error: None,
    })
}

/// Failure while syncing CVE List data.
#[derive(Debug)]
pub enum CveListSyncError {
    /// Failed to start sync-run metadata.
    StartSyncRun(CveListSyncRunError),
    /// Failed to access the CVE List Git repository.
    Git(CveListGitError),
    /// Failed to read or parse CVE List repository records.
    Repository(CveListRepositoryError),
    /// Failed to write parsed CVE List records.
    Storage(WriteCveListRecordsError),
    /// Failed to read sync-run metadata.
    SyncRunMetadata(CveListSyncRunError),
    /// Failed to finish a successful or not-modified sync run.
    FinishSyncRun(CveListSyncRunError),
    /// Failed to finish sync-run metadata after the sync operation failed.
    FinishFailedSyncRun {
        /// The original sync operation error message.
        sync_error: String,
        /// The failure while marking the sync run failed.
        finish_error: CveListSyncRunError,
    },
}

impl std::fmt::Display for CveListSyncError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StartSyncRun(_) => write!(f, "failed to start CVE List sync run"),
            Self::Git(_) => write!(f, "failed to access CVE List Git repository"),
            Self::Repository(_) => write!(f, "failed to read CVE List repository records"),
            Self::Storage(_) => write!(f, "failed to write CVE List records"),
            Self::SyncRunMetadata(_) => write!(f, "failed to read CVE List sync-run metadata"),
            Self::FinishSyncRun(_) => write!(f, "failed to finish CVE List sync run"),
            Self::FinishFailedSyncRun { sync_error, .. } => write!(
                f,
                "failed to mark failed CVE List sync run after sync error: {sync_error}"
            ),
        }
    }
}

impl std::error::Error for CveListSyncError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::StartSyncRun(err) | Self::SyncRunMetadata(err) | Self::FinishSyncRun(err) => {
                Some(err)
            }
            Self::Git(err) => Some(err),
            Self::Repository(err) => Some(err),
            Self::Storage(err) => Some(err),
            Self::FinishFailedSyncRun { finish_error, .. } => Some(finish_error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use camino::{Utf8Path, Utf8PathBuf};
    use sea_orm::{DbBackend, MockDatabase, MockExecResult, Value};
    use std::{
        collections::{BTreeMap, HashMap},
        sync::Mutex,
    };

    const OLD_COMMIT_SHA: &str = "0123456789abcdef0123456789abcdef01234567";
    const NEW_COMMIT_SHA: &str = "fedcba9876543210fedcba9876543210fedcba98";

    #[derive(Default)]
    struct MockCveListGit {
        all_paths: Vec<Utf8PathBuf>,
        changed_paths: Vec<Utf8PathBuf>,
        files: HashMap<Utf8PathBuf, String>,
        calls: Mutex<Vec<&'static str>>,
    }

    #[async_trait]
    impl CveListGit for MockCveListGit {
        async fn ensure_checkout(&self) -> Result<(), CveListGitError> {
            self.calls
                .lock()
                .expect("calls lock")
                .push("ensure_checkout");
            Ok(())
        }

        async fn fetch(&self) -> Result<(), CveListGitError> {
            self.calls.lock().expect("calls lock").push("fetch");
            Ok(())
        }

        async fn resolve_ref(&self, rev: &str) -> Result<CommitSha, CveListGitError> {
            assert_eq!(rev, "FETCH_HEAD");
            self.calls.lock().expect("calls lock").push("resolve_ref");
            CommitSha::parse(NEW_COMMIT_SHA)
        }

        async fn changed_cve_files(
            &self,
            old: &CommitSha,
            new: &CommitSha,
        ) -> Result<Vec<Utf8PathBuf>, CveListGitError> {
            assert_eq!(old.as_str(), OLD_COMMIT_SHA);
            assert_eq!(new.as_str(), NEW_COMMIT_SHA);
            self.calls.lock().expect("calls lock").push("changed_files");
            Ok(self.changed_paths.clone())
        }

        async fn all_cve_files(
            &self,
            commit: &CommitSha,
        ) -> Result<Vec<Utf8PathBuf>, CveListGitError> {
            assert_eq!(commit.as_str(), NEW_COMMIT_SHA);
            self.calls.lock().expect("calls lock").push("all_files");
            Ok(self.all_paths.clone())
        }

        async fn read_file_at_commit(
            &self,
            commit: &CommitSha,
            path: &Utf8Path,
        ) -> Result<String, CveListGitError> {
            assert_eq!(commit.as_str(), NEW_COMMIT_SHA);
            self.calls.lock().expect("calls lock").push("read_file");
            self.files
                .get(path)
                .cloned()
                .ok_or_else(|| CveListGitError::NonUtf8Path(format!("missing test file {path}")))
        }
    }

    #[test]
    fn sync_cve_list_once_imports_all_records_without_prior_success() {
        let records = [
            parsed_file_path("CVE-2026-1000"),
            parsed_file_path("CVE-2026-1001"),
        ];
        let git = MockCveListGit {
            all_paths: records.to_vec(),
            files: HashMap::from([
                (records[0].clone(), cve_record("CVE-2026-1000")),
                (records[1].clone(), cve_record("CVE-2026-1001")),
            ]),
            ..Default::default()
        };
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([
                vec![mock_generation_row(42)],
                Vec::<BTreeMap<String, Value>>::new(),
            ])
            .append_exec_results(mock_exec_results(2))
            .into_connection();

        let summary = run_async(sync_cve_list_once(
            &db,
            &git,
            &repository_url(),
            &repository_ref(),
            500,
        ))
        .expect("sync should succeed");

        assert_eq!(
            summary,
            CveListSyncSummary {
                generation: 42,
                status: CveListSyncRunStatus::Success,
                commit_sha: Some(CommitSha::parse(NEW_COMMIT_SHA).expect("valid commit")),
                records_seen: 2,
                records_inserted: 2,
                records_updated: 0,
            }
        );
        assert_eq!(
            git.calls.lock().expect("calls lock").as_slice(),
            [
                "ensure_checkout",
                "fetch",
                "resolve_ref",
                "all_files",
                "read_file",
                "read_file",
            ]
        );

        let transaction_log = db.into_transaction_log();
        assert!(transaction_log.iter().any(|entry| {
            entry.statements()[0]
                .sql
                .contains(r#"INSERT INTO "public"."cve_list_records""#)
        }));
        assert!(
            transaction_log
                .iter()
                .all(|entry| !entry.statements()[0].sql.contains("ON CONFLICT"))
        );
        assert!(transaction_log.iter().all(|entry| {
            !entry.statements()[0]
                .sql
                .contains("cve_list_records_staging")
        }));
        assert!(
            transaction_log
                .iter()
                .all(|entry| !entry.statements()[0].sql.contains("SELECT COUNT"))
        );
    }

    #[test]
    fn sync_cve_list_once_imports_changed_records_after_prior_success() {
        let path = parsed_file_path("CVE-2026-1000");
        let git = MockCveListGit {
            changed_paths: vec![path.clone()],
            files: HashMap::from([(path, cve_record("CVE-2026-1000"))]),
            ..Default::default()
        };
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([
                vec![mock_generation_row(42)],
                vec![mock_commit_sha_row(OLD_COMMIT_SHA)],
                vec![mock_existing_count_row(0)],
            ])
            .append_exec_results(mock_exec_results(5))
            .into_connection();

        let summary = run_async(sync_cve_list_once(
            &db,
            &git,
            &repository_url(),
            &repository_ref(),
            500,
        ))
        .expect("sync should succeed");

        assert_eq!(summary.status, CveListSyncRunStatus::Success);
        assert_eq!(summary.records_seen, 1);
        assert_eq!(
            git.calls.lock().expect("calls lock").as_slice(),
            [
                "ensure_checkout",
                "fetch",
                "resolve_ref",
                "changed_files",
                "read_file",
            ]
        );
    }

    #[test]
    fn sync_cve_list_once_marks_not_modified_when_commit_matches() {
        let git = MockCveListGit::default();
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([
                vec![mock_generation_row(42)],
                vec![mock_commit_sha_row(NEW_COMMIT_SHA)],
            ])
            .append_exec_results([MockExecResult {
                last_insert_id: 0,
                rows_affected: 1,
            }])
            .into_connection();

        let summary = run_async(sync_cve_list_once(
            &db,
            &git,
            &repository_url(),
            &repository_ref(),
            500,
        ))
        .expect("sync should succeed");

        assert_eq!(
            summary,
            CveListSyncSummary {
                generation: 42,
                status: CveListSyncRunStatus::NotModified,
                commit_sha: Some(CommitSha::parse(NEW_COMMIT_SHA).expect("valid commit")),
                records_seen: 0,
                records_inserted: 0,
                records_updated: 0,
            }
        );
        assert_eq!(
            git.calls.lock().expect("calls lock").as_slice(),
            ["ensure_checkout", "fetch", "resolve_ref"]
        );
    }

    #[test]
    fn sync_cve_list_once_marks_run_failed_after_repository_error() {
        let path = parsed_file_path("CVE-2026-1000");
        let git = MockCveListGit {
            all_paths: vec![path.clone()],
            files: HashMap::from([(path, "{".to_owned())]),
            ..Default::default()
        };
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([
                vec![mock_generation_row(42)],
                Vec::<BTreeMap<String, Value>>::new(),
            ])
            .append_exec_results([MockExecResult {
                last_insert_id: 0,
                rows_affected: 1,
            }])
            .into_connection();

        let err = run_async(sync_cve_list_once(
            &db,
            &git,
            &repository_url(),
            &repository_ref(),
            500,
        ))
        .expect_err("sync should fail");

        assert!(matches!(err, CveListSyncError::Repository(_)));
        let transaction_log = db.into_transaction_log();
        let finish_sql = &transaction_log[2].statements()[0].sql;
        assert!(finish_sql.contains(r#"UPDATE "public"."cve_list_sync_runs""#));
        assert!(finish_sql.contains(r#""status" = $"#));
        assert!(finish_sql.contains(r#""error" = $"#));
    }

    fn repository_url() -> Url {
        Url::parse("https://github.com/CVEProject/cvelistV5.git").expect("valid URL")
    }

    fn repository_ref() -> GitRef {
        GitRef::parse("main").expect("valid Git ref")
    }

    fn mock_generation_row(generation: i64) -> BTreeMap<String, Value> {
        BTreeMap::from([("generation".to_owned(), generation.into())])
    }

    fn mock_commit_sha_row(commit_sha: &str) -> BTreeMap<String, Value> {
        BTreeMap::from([("commit_sha".to_owned(), commit_sha.to_owned().into())])
    }

    fn mock_existing_count_row(existing: i64) -> BTreeMap<String, Value> {
        BTreeMap::from([("existing".to_owned(), existing.into())])
    }

    fn mock_exec_results(count: usize) -> Vec<MockExecResult> {
        (0..count)
            .map(|_| MockExecResult {
                last_insert_id: 0,
                rows_affected: 1,
            })
            .collect()
    }

    fn parsed_file_path(cve_id: &str) -> Utf8PathBuf {
        Utf8PathBuf::from(format!("cves/2026/1xxx/{cve_id}.json"))
    }

    fn cve_record(cve_id: &str) -> String {
        format!(
            r#"{{
                "dataType": "CVE_RECORD",
                "dataVersion": "5.2",
                "cveMetadata": {{
                    "cveId": "{cve_id}",
                    "state": "PUBLISHED"
                }}
            }}"#
        )
    }

    fn run_async<T>(future: impl std::future::Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime should build")
            .block_on(future)
    }
}
