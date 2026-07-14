//! Database storage for parsed CVE List records.

use crate::{
    cve::{
        git::{CommitSha, CveListGitError, GitRef},
        progress::{CveListSyncProgress, CveListSyncProgressReporter, NoopCveListSyncProgress},
        repository::ParsedCveListFile,
    },
    db::entities::{cve_list_record_staging, cve_list_records, cve_list_sync_runs},
};
use sea_orm::{
    ActiveValue::{NotSet, Set},
    ColumnTrait as _, ConnectionTrait, DatabaseBackend, DeriveIden, EntityTrait as _,
    PaginatorTrait as _, QueryFilter as _, QueryOrder as _, QuerySelect as _, Statement,
    sea_query::{Expr, ExprTrait as _, OnConflict, Query},
};
use std::{collections::HashSet, fmt};
use tokio::sync::mpsc;
use url::Url;

/// Default number of CVE List records to write per database batch.
pub const DEFAULT_CVE_LIST_RECORD_WRITE_BATCH_SIZE: usize = 500;

/// Default number of parsed CVE List records to buffer between parsing and writing.
pub const DEFAULT_CVE_LIST_RECORD_WRITE_CHANNEL_SIZE: usize = 1024;

/// PostgreSQL advisory-lock key used to serialize CVE List sync and reset work.
pub const CVE_LIST_SYNC_ADVISORY_LOCK_ID: i64 = 0x4356_454c_5359_4e43;

/// A CVE List sync-run status.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CveListSyncRunStatus {
    /// The sync is currently running.
    Running,
    /// The sync completed and stored records.
    Success,
    /// The sync failed.
    Failed,
    /// The configured Git ref resolved to the last successfully synced commit.
    NotModified,
}

impl fmt::Display for CveListSyncRunStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let status = match self {
            Self::Running => "running",
            Self::Success => "success",
            Self::Failed => "failed",
            Self::NotModified => "not_modified",
        };

        f.write_str(status)
    }
}

impl CveListSyncRunStatus {
    fn is_terminal(self) -> bool {
        match self {
            Self::Running => false,
            Self::Success | Self::Failed | Self::NotModified => true,
        }
    }
}

/// Completion metadata for a CVE List sync run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FinishCveListSyncRun {
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
    /// Error message for failed runs.
    pub error: Option<String>,
}

/// Counts from writing CVE List records to storage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WriteCveListRecordsSummary {
    /// Number of parsed records supplied to the write.
    pub records_seen: usize,
    /// Number of supplied records that did not already exist.
    pub records_inserted: usize,
    /// Number of supplied records that already existed and were updated.
    pub records_updated: usize,
}

/// Counts from resetting CVE List database storage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResetCveListStorageSummary {
    /// Number of stored CVE List records removed.
    pub records_deleted: u64,
    /// Number of CVE List sync-run records removed.
    pub sync_runs_deleted: u64,
}

/// Counts from marking stale running CVE List sync runs failed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FailRunningCveListSyncRunsSummary {
    /// Number of running sync runs marked failed.
    pub sync_runs_failed: u64,
}

/// Upsert parsed CVE List records and return insert/update counts.
///
/// Inserted rows use the database defaults for `first_seen_at`, `last_seen_at`,
/// and `updated_at`. Updated rows preserve `first_seen_at` and refresh
/// `last_seen_at` and `updated_at` to the database current timestamp.
pub async fn write_cve_list_records<C>(
    db: &C,
    records: &[ParsedCveListFile],
) -> Result<WriteCveListRecordsSummary, WriteCveListRecordsError>
where
    C: ConnectionTrait,
{
    write_cve_list_records_with_batch_size(db, records, DEFAULT_CVE_LIST_RECORD_WRITE_BATCH_SIZE)
        .await
}

/// Mark CVE List records deleted without removing their stored payloads.
pub async fn mark_deleted_cve_list_records<C>(
    db: &C,
    cve_ids: &[String],
) -> Result<WriteCveListRecordsSummary, WriteCveListRecordsError>
where
    C: ConnectionTrait,
{
    if cve_ids.is_empty() {
        return Ok(WriteCveListRecordsSummary {
            records_seen: 0,
            records_inserted: 0,
            records_updated: 0,
        });
    }

    collect_unique_ids(cve_ids)?;
    let result = cve_list_records::Entity::update_many()
        .col_expr(cve_list_records::Column::Deleted, Expr::value(true))
        .col_expr(
            cve_list_records::Column::UpdatedAt,
            Expr::current_timestamp(),
        )
        .filter(cve_list_records::Column::CveId.is_in(cve_ids.iter().cloned()))
        .exec(db)
        .await
        .map_err(WriteCveListRecordsError::Db)?;
    let records_updated = usize::try_from(result.rows_affected)
        .map_err(|_| WriteCveListRecordsError::CountTooLarge(result.rows_affected))?;

    Ok(WriteCveListRecordsSummary {
        records_seen: cve_ids.len(),
        records_inserted: 0,
        records_updated,
    })
}

/// Mark active records missing from a staged full snapshot as deleted.
pub async fn mark_cve_list_records_missing_from_staged_snapshot<C>(
    db: &C,
    generation: i64,
) -> Result<WriteCveListRecordsSummary, WriteCveListRecordsError>
where
    C: ConnectionTrait,
{
    let mut staged_cve_ids = Query::select();
    staged_cve_ids
        .column(CveListRecordStaging::CveId)
        .from((CveListSchema::Public, CveListRecordStaging::Table))
        .and_where(Expr::col(CveListRecordStaging::Generation).eq(generation));

    let result = cve_list_records::Entity::update_many()
        .col_expr(cve_list_records::Column::Deleted, Expr::value(true))
        .col_expr(
            cve_list_records::Column::UpdatedAt,
            Expr::current_timestamp(),
        )
        .filter(cve_list_records::Column::Deleted.eq(false))
        .filter(cve_list_records::Column::CveId.not_in_subquery(staged_cve_ids))
        .exec(db)
        .await
        .map_err(WriteCveListRecordsError::Db)?;
    let records_updated = usize::try_from(result.rows_affected)
        .map_err(|_| WriteCveListRecordsError::CountTooLarge(result.rows_affected))?;

    Ok(WriteCveListRecordsSummary {
        records_seen: records_updated,
        records_inserted: 0,
        records_updated,
    })
}

/// Upsert parsed CVE List records in batches and return insert/update counts.
pub async fn write_cve_list_records_with_batch_size<C>(
    db: &C,
    records: &[ParsedCveListFile],
    batch_size: usize,
) -> Result<WriteCveListRecordsSummary, WriteCveListRecordsError>
where
    C: ConnectionTrait,
{
    write_cve_list_records_with_batch_size_and_progress(
        db,
        records,
        batch_size,
        &NoopCveListSyncProgress,
    )
    .await
}

/// Upsert parsed CVE List records in batches, reporting progress.
pub async fn write_cve_list_records_with_batch_size_and_progress<C, P>(
    db: &C,
    records: &[ParsedCveListFile],
    batch_size: usize,
    progress: &P,
) -> Result<WriteCveListRecordsSummary, WriteCveListRecordsError>
where
    C: ConnectionTrait,
    P: CveListSyncProgressReporter + ?Sized,
{
    if records.is_empty() {
        return Ok(WriteCveListRecordsSummary {
            records_seen: 0,
            records_inserted: 0,
            records_updated: 0,
        });
    }

    if batch_size == 0 {
        return Err(WriteCveListRecordsError::InvalidBatchSize);
    }

    collect_unique_cve_ids(records)?;

    progress.report(CveListSyncProgress::RecordWriteStarted {
        records: records.len(),
    });
    let mut summary = WriteCveListRecordsSummary {
        records_seen: 0,
        records_inserted: 0,
        records_updated: 0,
    };
    for chunk in records.chunks(batch_size) {
        write_cve_list_record_batch(db, chunk, records.len(), &mut summary, progress).await?;
    }

    Ok(summary)
}

/// Upsert parsed CVE List records from a channel in batches, reporting progress.
pub async fn write_cve_list_records_from_receiver_with_batch_size_and_progress<C, P>(
    db: &C,
    mut records: mpsc::Receiver<ParsedCveListFile>,
    batch_size: usize,
    total_records: usize,
    progress: &P,
) -> Result<WriteCveListRecordsSummary, WriteCveListRecordsError>
where
    C: ConnectionTrait,
    P: CveListSyncProgressReporter + ?Sized,
{
    if batch_size == 0 {
        return Err(WriteCveListRecordsError::InvalidBatchSize);
    }

    let mut seen = HashSet::new();
    let mut batch = Vec::with_capacity(batch_size);
    let mut summary = WriteCveListRecordsSummary {
        records_seen: 0,
        records_inserted: 0,
        records_updated: 0,
    };
    let mut write_started = false;

    while let Some(record) = records.recv().await {
        if !write_started {
            progress.report(CveListSyncProgress::RecordWriteStarted {
                records: total_records,
            });
            write_started = true;
        }

        let cve_id = record.record.cve_id.as_str().to_owned();
        if !seen.insert(cve_id.clone()) {
            return Err(WriteCveListRecordsError::DuplicateCveId(cve_id));
        }

        batch.push(record);
        if batch.len() == batch_size {
            write_cve_list_record_batch(db, &batch, total_records, &mut summary, progress).await?;
            batch.clear();
        }
    }

    if !batch.is_empty() {
        write_cve_list_record_batch(db, &batch, total_records, &mut summary, progress).await?;
    }

    Ok(summary)
}

/// Stage parsed CVE List records from a channel in batches, reporting progress.
///
/// Staged records are not visible to readers until
/// [`merge_staged_cve_list_records`] publishes them into the live records table.
pub async fn stage_cve_list_records_from_receiver_with_batch_size_and_progress<C, P>(
    db: &C,
    generation: i64,
    mut records: mpsc::Receiver<ParsedCveListFile>,
    batch_size: usize,
    total_records: usize,
    progress: &P,
) -> Result<WriteCveListRecordsSummary, WriteCveListRecordsError>
where
    C: ConnectionTrait,
    P: CveListSyncProgressReporter + ?Sized,
{
    if batch_size == 0 {
        return Err(WriteCveListRecordsError::InvalidBatchSize);
    }

    let mut seen = HashSet::new();
    let mut batch = Vec::with_capacity(batch_size);
    let mut summary = WriteCveListRecordsSummary {
        records_seen: 0,
        records_inserted: 0,
        records_updated: 0,
    };
    let mut write_started = false;

    while let Some(record) = records.recv().await {
        if !write_started {
            progress.report(CveListSyncProgress::RecordWriteStarted {
                records: total_records,
            });
            write_started = true;
        }

        let cve_id = record.record.cve_id.as_str().to_owned();
        if !seen.insert(cve_id.clone()) {
            return Err(WriteCveListRecordsError::DuplicateCveId(cve_id));
        }

        batch.push(record);
        if batch.len() == batch_size {
            stage_cve_list_record_batch(
                db,
                generation,
                &batch,
                total_records,
                &mut summary,
                progress,
            )
            .await?;
            batch.clear();
        }
    }

    if !batch.is_empty() {
        stage_cve_list_record_batch(
            db,
            generation,
            &batch,
            total_records,
            &mut summary,
            progress,
        )
        .await?;
    }

    Ok(summary)
}

/// Insert new parsed CVE List records from a channel in batches, reporting progress.
///
/// This is intended for true first-run imports where CVE List storage is empty.
/// It avoids staging-table merge work and fails on database uniqueness conflicts
/// rather than converting them into updates.
pub async fn insert_new_cve_list_records_from_receiver_with_batch_size_and_progress<C, P>(
    db: &C,
    mut records: mpsc::Receiver<ParsedCveListFile>,
    batch_size: usize,
    total_records: usize,
    progress: &P,
) -> Result<WriteCveListRecordsSummary, WriteCveListRecordsError>
where
    C: ConnectionTrait,
    P: CveListSyncProgressReporter + ?Sized,
{
    if batch_size == 0 {
        return Err(WriteCveListRecordsError::InvalidBatchSize);
    }

    let mut seen = HashSet::new();
    let mut batch = Vec::with_capacity(batch_size);
    let mut summary = WriteCveListRecordsSummary {
        records_seen: 0,
        records_inserted: 0,
        records_updated: 0,
    };
    let mut write_started = false;

    while let Some(record) = records.recv().await {
        if !write_started {
            progress.report(CveListSyncProgress::RecordWriteStarted {
                records: total_records,
            });
            write_started = true;
        }

        let cve_id = record.record.cve_id.as_str().to_owned();
        if !seen.insert(cve_id.clone()) {
            return Err(WriteCveListRecordsError::DuplicateCveId(cve_id));
        }

        batch.push(record);
        if batch.len() == batch_size {
            insert_new_cve_list_record_batch(db, &batch, total_records, &mut summary, progress)
                .await?;
            batch.clear();
        }
    }

    if !batch.is_empty() {
        insert_new_cve_list_record_batch(db, &batch, total_records, &mut summary, progress).await?;
    }

    Ok(summary)
}

/// Create a running CVE List sync run.
pub async fn start_cve_list_sync_run<C>(
    db: &C,
    repository_url: &Url,
    repository_ref: &GitRef,
) -> Result<i64, CveListSyncRunError>
where
    C: ConnectionTrait,
{
    let run = cve_list_sync_runs::ActiveModel {
        generation: NotSet,
        checked_at: NotSet,
        completed_at: NotSet,
        status: Set(CveListSyncRunStatus::Running.to_string()),
        repository_url: Set(Some(repository_url.as_str().to_owned())),
        repository_ref: Set(Some(repository_ref.as_str().to_owned())),
        commit_sha: Set(None),
        records_seen: Set(0),
        records_inserted: Set(0),
        records_updated: Set(0),
        error: Set(None),
    };

    cve_list_sync_runs::Entity::insert(run)
        .exec(db)
        .await
        .map(|result| result.last_insert_id)
        .map_err(CveListSyncRunError::Db)
}

/// Mark a CVE List sync run as completed.
pub async fn finish_cve_list_sync_run<C>(
    db: &C,
    generation: i64,
    completion: FinishCveListSyncRun,
) -> Result<(), CveListSyncRunError>
where
    C: ConnectionTrait,
{
    if !completion.status.is_terminal() {
        return Err(CveListSyncRunError::NonTerminalStatus(completion.status));
    }

    let records_seen = count_to_i32(completion.records_seen, "records_seen")?;
    let records_inserted = count_to_i32(completion.records_inserted, "records_inserted")?;
    let records_updated = count_to_i32(completion.records_updated, "records_updated")?;
    let commit_sha = completion
        .commit_sha
        .as_ref()
        .map(|commit_sha| commit_sha.as_str().to_owned());

    let result = cve_list_sync_runs::Entity::update_many()
        .col_expr(
            cve_list_sync_runs::Column::CompletedAt,
            Expr::current_timestamp(),
        )
        .col_expr(
            cve_list_sync_runs::Column::Status,
            Expr::value(completion.status.to_string()),
        )
        .col_expr(
            cve_list_sync_runs::Column::CommitSha,
            Expr::value(commit_sha),
        )
        .col_expr(
            cve_list_sync_runs::Column::RecordsSeen,
            Expr::value(records_seen),
        )
        .col_expr(
            cve_list_sync_runs::Column::RecordsInserted,
            Expr::value(records_inserted),
        )
        .col_expr(
            cve_list_sync_runs::Column::RecordsUpdated,
            Expr::value(records_updated),
        )
        .col_expr(
            cve_list_sync_runs::Column::Error,
            Expr::value(completion.error),
        )
        .filter(cve_list_sync_runs::Column::Generation.eq(generation))
        .exec(db)
        .await
        .map_err(CveListSyncRunError::Db)?;

    if result.rows_affected == 0 {
        return Err(CveListSyncRunError::SyncRunNotFound(generation));
    }

    Ok(())
}

/// Return the commit SHA from the most recent successful CVE List sync run.
pub async fn last_successful_cve_list_sync_commit<C>(
    db: &C,
) -> Result<Option<CommitSha>, CveListSyncRunError>
where
    C: ConnectionTrait,
{
    let commit_sha = cve_list_sync_runs::Entity::find()
        .select_only()
        .column(cve_list_sync_runs::Column::CommitSha)
        .filter(cve_list_sync_runs::Column::Status.eq(CveListSyncRunStatus::Success.to_string()))
        .filter(cve_list_sync_runs::Column::CommitSha.is_not_null())
        .order_by_desc(cve_list_sync_runs::Column::Generation)
        .limit(1)
        .into_tuple::<Option<String>>()
        .one(db)
        .await
        .map_err(CveListSyncRunError::Db)?
        .flatten();

    commit_sha
        .as_deref()
        .map(CommitSha::parse)
        .transpose()
        .map_err(CveListSyncRunError::InvalidStoredCommitSha)
}

/// Return the commit SHA from the most recent successful CVE List sync run for a source.
pub async fn last_successful_cve_list_sync_commit_for_source<C>(
    db: &C,
    repository_url: &Url,
    repository_ref: &GitRef,
) -> Result<Option<CommitSha>, CveListSyncRunError>
where
    C: ConnectionTrait,
{
    let commit_sha = cve_list_sync_runs::Entity::find()
        .select_only()
        .column(cve_list_sync_runs::Column::CommitSha)
        .filter(cve_list_sync_runs::Column::Status.eq(CveListSyncRunStatus::Success.to_string()))
        .filter(cve_list_sync_runs::Column::RepositoryUrl.eq(repository_url.as_str()))
        .filter(cve_list_sync_runs::Column::RepositoryRef.eq(repository_ref.as_str()))
        .filter(cve_list_sync_runs::Column::CommitSha.is_not_null())
        .order_by_desc(cve_list_sync_runs::Column::Generation)
        .limit(1)
        .into_tuple::<Option<String>>()
        .one(db)
        .await
        .map_err(CveListSyncRunError::Db)?
        .flatten();

    commit_sha
        .as_deref()
        .map(CommitSha::parse)
        .transpose()
        .map_err(CveListSyncRunError::InvalidStoredCommitSha)
}

/// Return the most recent CVE List sync run.
pub async fn latest_cve_list_sync_run<C>(
    db: &C,
) -> Result<Option<cve_list_sync_runs::Model>, CveListSyncRunError>
where
    C: ConnectionTrait,
{
    cve_list_sync_runs::Entity::find()
        .order_by_desc(cve_list_sync_runs::Column::Generation)
        .limit(1)
        .one(db)
        .await
        .map_err(CveListSyncRunError::Db)
}

/// Return whether any active CVE List records are already stored.
pub async fn has_cve_list_records<C>(db: &C) -> Result<bool, CveListRecordLookupError>
where
    C: ConnectionTrait,
{
    let cve_id = cve_list_records::Entity::find()
        .select_only()
        .column(cve_list_records::Column::CveId)
        .filter(cve_list_records::Column::Deleted.eq(false))
        .limit(1)
        .into_tuple::<String>()
        .one(db)
        .await
        .map_err(CveListRecordLookupError::Db)?;

    Ok(cve_id.is_some())
}

/// Publish staged CVE List records for a sync run into the live records table.
pub async fn merge_staged_cve_list_records<C>(
    db: &C,
    generation: i64,
) -> Result<(), WriteCveListRecordsError>
where
    C: ConnectionTrait,
{
    let mut select = Query::select();
    select
        .columns([
            CveListRecordStaging::CveId,
            CveListRecordStaging::RecordFormatVersion,
            CveListRecordStaging::Record,
        ])
        .expr(Expr::value(false))
        .from((CveListSchema::Public, CveListRecordStaging::Table))
        .and_where(Expr::col(CveListRecordStaging::Generation).eq(generation));

    let mut merge = Query::insert();
    merge
        .into_table((CveListSchema::Public, CveListRecords::Table))
        .columns([
            CveListRecords::CveId,
            CveListRecords::RecordFormatVersion,
            CveListRecords::Record,
            CveListRecords::Deleted,
        ])
        .select_from(select)
        .map_err(|err| WriteCveListRecordsError::QueryBuild(err.to_string()))?
        .on_conflict(
            OnConflict::column(CveListRecords::CveId)
                .update_columns([CveListRecords::RecordFormatVersion, CveListRecords::Record])
                .values([
                    (CveListRecords::Deleted, Expr::value(false)),
                    (CveListRecords::LastSeenAt, Expr::current_timestamp()),
                    (CveListRecords::UpdatedAt, Expr::current_timestamp()),
                ])
                .to_owned(),
        );

    db.execute(&merge)
        .await
        .map(|_| ())
        .map_err(WriteCveListRecordsError::Db)?;

    clear_staged_cve_list_records(db, generation).await
}

/// Remove staged CVE List records for a sync run.
pub async fn clear_staged_cve_list_records<C>(
    db: &C,
    generation: i64,
) -> Result<(), WriteCveListRecordsError>
where
    C: ConnectionTrait,
{
    cve_list_record_staging::Entity::delete_many()
        .filter(cve_list_record_staging::Column::Generation.eq(generation))
        .exec(db)
        .await
        .map(|_| ())
        .map_err(WriteCveListRecordsError::Db)
}

/// Clear CVE List records and sync-run metadata from database storage.
///
/// Clearing sync-run metadata is part of the reset because the latest
/// successful sync commit determines whether future syncs can use the
/// incremental path.
///
/// Callers must pass an active transaction so the transaction-scoped advisory
/// lock is held until the reset is committed or rolled back.
pub async fn reset_cve_list_storage<C>(
    db: &C,
    force: bool,
) -> Result<ResetCveListStorageSummary, ResetCveListStorageError>
where
    C: ConnectionTrait,
{
    let sync_lock_acquired = try_acquire_cve_list_sync_lock(db)
        .await
        .map_err(ResetCveListStorageError::SyncLock)?;
    if !sync_lock_acquired {
        return Err(ResetCveListStorageError::SyncAlreadyRunning);
    }

    let running_sync_runs = count_running_cve_list_sync_runs(db).await?;
    if running_sync_runs > 0 && !force {
        return Err(ResetCveListStorageError::RunningSyncRuns(running_sync_runs));
    }

    let records_deleted = cve_list_records::Entity::find()
        .count(db)
        .await
        .map_err(ResetCveListStorageError::Db)?;
    let sync_runs_deleted = cve_list_sync_runs::Entity::find()
        .count(db)
        .await
        .map_err(ResetCveListStorageError::Db)?;
    let truncate = Statement::from_string(
        db.get_database_backend(),
        "TRUNCATE TABLE public.cve_list_records, public.cve_list_record_staging, \
         public.cve_list_sync_runs RESTART IDENTITY"
            .to_owned(),
    );

    db.execute_raw(truncate)
        .await
        .map(|_| ResetCveListStorageSummary {
            records_deleted,
            sync_runs_deleted,
        })
        .map_err(ResetCveListStorageError::Db)
}

/// Try to acquire the transaction-scoped CVE List sync advisory lock.
///
/// Callers that need the lock to guard multiple statements must pass an active
/// transaction. Passing a plain connection only guards the current statement.
pub async fn try_acquire_cve_list_sync_lock<C>(db: &C) -> Result<bool, CveListSyncRunError>
where
    C: ConnectionTrait,
{
    let statement = Statement::from_string(
        DatabaseBackend::Postgres,
        format!("SELECT pg_try_advisory_xact_lock({CVE_LIST_SYNC_ADVISORY_LOCK_ID})"),
    );

    db.query_one_raw(statement)
        .await
        .map_err(CveListSyncRunError::Db)?
        .ok_or(CveListSyncRunError::MissingAdvisoryLockResult)?
        .try_get_by_index(0)
        .map_err(CveListSyncRunError::Db)
}

/// Mark running CVE List sync runs failed.
///
/// Callers should only use this after verifying no live sync holds the
/// process-scoped advisory lock.
pub async fn fail_running_cve_list_sync_runs<C>(
    db: &C,
    error: &str,
) -> Result<FailRunningCveListSyncRunsSummary, CveListSyncRunError>
where
    C: ConnectionTrait,
{
    let result = cve_list_sync_runs::Entity::update_many()
        .col_expr(
            cve_list_sync_runs::Column::CompletedAt,
            Expr::current_timestamp(),
        )
        .col_expr(
            cve_list_sync_runs::Column::Status,
            Expr::value(CveListSyncRunStatus::Failed.to_string()),
        )
        .col_expr(cve_list_sync_runs::Column::Error, Expr::value(error))
        .filter(cve_list_sync_runs::Column::Status.eq(CveListSyncRunStatus::Running.to_string()))
        .exec(db)
        .await
        .map_err(CveListSyncRunError::Db)?;

    Ok(FailRunningCveListSyncRunsSummary {
        sync_runs_failed: result.rows_affected,
    })
}

async fn count_running_cve_list_sync_runs<C>(db: &C) -> Result<u64, ResetCveListStorageError>
where
    C: ConnectionTrait,
{
    cve_list_sync_runs::Entity::find()
        .filter(cve_list_sync_runs::Column::Status.eq(CveListSyncRunStatus::Running.to_string()))
        .count(db)
        .await
        .map_err(ResetCveListStorageError::Db)
}

fn count_to_i32(count: usize, field: &'static str) -> Result<i32, CveListSyncRunError> {
    i32::try_from(count).map_err(|_| CveListSyncRunError::CountTooLarge { field, count })
}

fn collect_unique_cve_ids(
    records: &[ParsedCveListFile],
) -> Result<Vec<String>, WriteCveListRecordsError> {
    let mut cve_ids = Vec::with_capacity(records.len());

    for record in records {
        cve_ids.push(record.record.cve_id.as_str().to_owned());
    }

    collect_unique_ids(&cve_ids)?;

    Ok(cve_ids)
}

fn collect_unique_ids(cve_ids: &[String]) -> Result<(), WriteCveListRecordsError> {
    let mut seen = HashSet::with_capacity(cve_ids.len());

    for cve_id in cve_ids {
        if !seen.insert(cve_id.clone()) {
            return Err(WriteCveListRecordsError::DuplicateCveId(cve_id.clone()));
        }
    }

    Ok(())
}

async fn write_cve_list_record_batch<C, P>(
    db: &C,
    records: &[ParsedCveListFile],
    total_records: usize,
    summary: &mut WriteCveListRecordsSummary,
    progress: &P,
) -> Result<(), WriteCveListRecordsError>
where
    C: ConnectionTrait,
    P: CveListSyncProgressReporter + ?Sized,
{
    let cve_ids = collect_unique_cve_ids(records)?;
    progress.report(CveListSyncProgress::ExistingRecordLookupStarted {
        records: cve_ids.len(),
    });
    let records_updated = count_existing_cve_records(db, &cve_ids).await?;
    progress.report(CveListSyncProgress::ExistingRecordLookupCompleted {
        records: records_updated,
    });

    upsert_cve_list_record_batch(db, records).await?;

    let records_inserted = cve_ids
        .len()
        .checked_sub(records_updated)
        .expect("existing CVE IDs were selected from supplied CVE IDs");

    summary.records_seen = summary
        .records_seen
        .checked_add(records.len())
        .expect("seen record count cannot exceed streamed record count");
    summary.records_inserted = summary
        .records_inserted
        .checked_add(records_inserted)
        .expect("inserted record count cannot exceed streamed record count");
    summary.records_updated = summary
        .records_updated
        .checked_add(records_updated)
        .expect("updated record count cannot exceed streamed record count");

    progress.report(CveListSyncProgress::RecordWriteBatchCompleted {
        written: summary.records_seen,
        total: total_records,
    });

    Ok(())
}

async fn stage_cve_list_record_batch<C, P>(
    db: &C,
    generation: i64,
    records: &[ParsedCveListFile],
    total_records: usize,
    summary: &mut WriteCveListRecordsSummary,
    progress: &P,
) -> Result<(), WriteCveListRecordsError>
where
    C: ConnectionTrait,
    P: CveListSyncProgressReporter + ?Sized,
{
    let cve_ids = collect_unique_cve_ids(records)?;
    progress.report(CveListSyncProgress::ExistingRecordLookupStarted {
        records: cve_ids.len(),
    });
    let records_updated = count_existing_cve_records(db, &cve_ids).await?;
    progress.report(CveListSyncProgress::ExistingRecordLookupCompleted {
        records: records_updated,
    });

    insert_cve_list_record_staging_batch(db, generation, records).await?;

    let records_inserted = cve_ids
        .len()
        .checked_sub(records_updated)
        .expect("existing CVE IDs were selected from supplied CVE IDs");

    summary.records_seen = summary
        .records_seen
        .checked_add(records.len())
        .expect("seen record count cannot exceed streamed record count");
    summary.records_inserted = summary
        .records_inserted
        .checked_add(records_inserted)
        .expect("inserted record count cannot exceed streamed record count");
    summary.records_updated = summary
        .records_updated
        .checked_add(records_updated)
        .expect("updated record count cannot exceed streamed record count");

    progress.report(CveListSyncProgress::RecordWriteBatchCompleted {
        written: summary.records_seen,
        total: total_records,
    });

    Ok(())
}

async fn insert_new_cve_list_record_batch<C, P>(
    db: &C,
    records: &[ParsedCveListFile],
    total_records: usize,
    summary: &mut WriteCveListRecordsSummary,
    progress: &P,
) -> Result<(), WriteCveListRecordsError>
where
    C: ConnectionTrait,
    P: CveListSyncProgressReporter + ?Sized,
{
    insert_cve_list_record_batch(db, records).await?;

    summary.records_seen = summary
        .records_seen
        .checked_add(records.len())
        .expect("seen record count cannot exceed streamed record count");
    summary.records_inserted = summary
        .records_inserted
        .checked_add(records.len())
        .expect("inserted record count cannot exceed streamed record count");

    progress.report(CveListSyncProgress::RecordWriteBatchCompleted {
        written: summary.records_seen,
        total: total_records,
    });

    Ok(())
}

async fn insert_cve_list_record_staging_batch<C>(
    db: &C,
    generation: i64,
    records: &[ParsedCveListFile],
) -> Result<(), WriteCveListRecordsError>
where
    C: ConnectionTrait,
{
    let mut insert = Query::insert();
    insert
        .into_table((CveListSchema::Public, CveListRecordStaging::Table))
        .columns([
            CveListRecordStaging::Generation,
            CveListRecordStaging::CveId,
            CveListRecordStaging::RecordFormatVersion,
            CveListRecordStaging::Record,
        ]);

    for record in records {
        insert
            .values([
                Expr::val(generation),
                Expr::val(record.record.cve_id.as_str().to_owned()),
                Expr::val(record.record.record_format_version.as_str().to_owned()),
                Expr::val(record.record.record.clone()),
            ])
            .map_err(|err| WriteCveListRecordsError::QueryBuild(err.to_string()))?;
    }

    db.execute(&insert)
        .await
        .map(|_| ())
        .map_err(WriteCveListRecordsError::Db)
}

async fn insert_cve_list_record_batch<C>(
    db: &C,
    records: &[ParsedCveListFile],
) -> Result<(), WriteCveListRecordsError>
where
    C: ConnectionTrait,
{
    let mut insert = Query::insert();
    insert
        .into_table((CveListSchema::Public, CveListRecords::Table))
        .columns([
            CveListRecords::CveId,
            CveListRecords::RecordFormatVersion,
            CveListRecords::Record,
            CveListRecords::Deleted,
        ]);

    for record in records {
        insert
            .values([
                Expr::val(record.record.cve_id.as_str().to_owned()),
                Expr::val(record.record.record_format_version.as_str().to_owned()),
                Expr::val(record.record.record.clone()),
                Expr::val(false),
            ])
            .map_err(|err| WriteCveListRecordsError::QueryBuild(err.to_string()))?;
    }

    db.execute(&insert)
        .await
        .map(|_| ())
        .map_err(WriteCveListRecordsError::Db)
}

async fn count_existing_cve_records<C>(
    db: &C,
    cve_ids: &[String],
) -> Result<usize, WriteCveListRecordsError>
where
    C: ConnectionTrait,
{
    let count = cve_list_records::Entity::find()
        .filter(cve_list_records::Column::CveId.is_in(cve_ids.iter().cloned()))
        .count(db)
        .await
        .map_err(WriteCveListRecordsError::Db)?;

    usize::try_from(count).map_err(|_| WriteCveListRecordsError::CountTooLarge(count))
}

async fn upsert_cve_list_record_batch<C>(
    db: &C,
    records: &[ParsedCveListFile],
) -> Result<(), WriteCveListRecordsError>
where
    C: ConnectionTrait,
{
    let mut insert = Query::insert();
    insert
        .into_table((CveListSchema::Public, CveListRecords::Table))
        .columns([
            CveListRecords::CveId,
            CveListRecords::RecordFormatVersion,
            CveListRecords::Record,
            CveListRecords::Deleted,
        ])
        .on_conflict(
            OnConflict::column(CveListRecords::CveId)
                .update_columns([
                    CveListRecords::RecordFormatVersion,
                    CveListRecords::Record,
                    CveListRecords::Deleted,
                ])
                .values([
                    (CveListRecords::LastSeenAt, Expr::current_timestamp()),
                    (CveListRecords::UpdatedAt, Expr::current_timestamp()),
                ])
                .to_owned(),
        );

    for record in records {
        insert
            .values([
                Expr::val(record.record.cve_id.as_str().to_owned()),
                Expr::val(record.record.record_format_version.as_str().to_owned()),
                Expr::val(record.record.record.clone()),
                Expr::val(false),
            ])
            .map_err(|err| WriteCveListRecordsError::QueryBuild(err.to_string()))?;
    }

    db.execute(&insert)
        .await
        .map(|_| ())
        .map_err(WriteCveListRecordsError::Db)
}

#[derive(DeriveIden)]
enum CveListSchema {
    Public,
}

#[derive(DeriveIden)]
enum CveListRecordStaging {
    Table,
    Generation,
    CveId,
    RecordFormatVersion,
    Record,
}

#[derive(DeriveIden)]
enum CveListRecords {
    Table,
    CveId,
    RecordFormatVersion,
    Record,
    Deleted,
    LastSeenAt,
    UpdatedAt,
}

/// Failure while writing CVE List records to storage.
#[derive(Debug)]
pub enum WriteCveListRecordsError {
    /// An existing-record count exceeded platform storage limits.
    CountTooLarge(u64),
    /// A database operation failed.
    Db(sea_orm::DbErr),
    /// The batch contained the same CVE ID more than once.
    DuplicateCveId(String),
    /// The configured write batch size was zero.
    InvalidBatchSize,
    /// The configured parse-to-write channel size was zero.
    InvalidWriteChannelSize,
    /// A SeaQuery statement could not be built.
    QueryBuild(String),
}

impl std::fmt::Display for WriteCveListRecordsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CountTooLarge(count) => {
                write!(f, "CVE List existing-record count is too large: {count}")
            }
            Self::Db(_) => write!(f, "failed to write CVE List records"),
            Self::DuplicateCveId(cve_id) => {
                write!(
                    f,
                    "CVE List record batch contains duplicate CVE ID {cve_id}"
                )
            }
            Self::InvalidBatchSize => write!(f, "CVE List write batch size must be greater than 0"),
            Self::InvalidWriteChannelSize => {
                write!(f, "CVE List write channel size must be greater than 0")
            }
            Self::QueryBuild(_) => write!(f, "failed to build CVE List storage query"),
        }
    }
}

impl std::error::Error for WriteCveListRecordsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::CountTooLarge(_) => None,
            Self::Db(err) => Some(err),
            Self::DuplicateCveId(_)
            | Self::InvalidBatchSize
            | Self::InvalidWriteChannelSize
            | Self::QueryBuild(_) => None,
        }
    }
}

/// Failure while updating CVE List sync-run metadata.
#[derive(Debug)]
pub enum CveListSyncRunError {
    /// A record count exceeded the sync-run table column range.
    CountTooLarge { field: &'static str, count: usize },
    /// A database operation failed.
    Db(sea_orm::DbErr),
    /// A stored commit SHA was invalid.
    InvalidStoredCommitSha(CveListGitError),
    /// The advisory-lock query returned no row.
    MissingAdvisoryLockResult,
    /// A non-terminal status was passed when completing a sync run.
    NonTerminalStatus(CveListSyncRunStatus),
    /// A CVE List sync run is already running.
    SyncAlreadyRunning,
    /// The sync run to complete was not found.
    SyncRunNotFound(i64),
}

impl std::fmt::Display for CveListSyncRunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CountTooLarge { field, count } => {
                write!(f, "CVE List sync-run count {field} is too large: {count}")
            }
            Self::Db(_) => write!(f, "failed to access CVE List sync-run metadata"),
            Self::InvalidStoredCommitSha(_) => {
                write!(f, "CVE List sync-run stored an invalid commit SHA")
            }
            Self::MissingAdvisoryLockResult => {
                write!(f, "CVE List sync advisory-lock query returned no row")
            }
            Self::NonTerminalStatus(status) => {
                write!(f, "CVE List sync-run status {status} is not terminal")
            }
            Self::SyncAlreadyRunning => {
                write!(f, "a CVE List sync run is already running")
            }
            Self::SyncRunNotFound(generation) => {
                write!(f, "CVE List sync run {generation} was not found")
            }
        }
    }
}

impl std::error::Error for CveListSyncRunError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Db(err) => Some(err),
            Self::InvalidStoredCommitSha(err) => Some(err),
            Self::CountTooLarge { .. }
            | Self::MissingAdvisoryLockResult
            | Self::NonTerminalStatus(_)
            | Self::SyncAlreadyRunning
            | Self::SyncRunNotFound(_) => None,
        }
    }
}

/// Failure while looking up stored CVE List records.
#[derive(Debug)]
pub enum CveListRecordLookupError {
    /// A database operation failed.
    Db(sea_orm::DbErr),
}

impl std::fmt::Display for CveListRecordLookupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Db(_) => write!(f, "failed to look up stored CVE List records"),
        }
    }
}

impl std::error::Error for CveListRecordLookupError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Db(err) => Some(err),
        }
    }
}

/// Failure while resetting CVE List database storage.
#[derive(Debug)]
pub enum ResetCveListStorageError {
    /// A database operation failed.
    Db(sea_orm::DbErr),
    /// One or more CVE List sync runs are still marked running.
    RunningSyncRuns(u64),
    /// Another session holds the CVE List sync advisory lock.
    SyncAlreadyRunning,
    /// The sync advisory-lock check failed.
    SyncLock(CveListSyncRunError),
}

impl std::fmt::Display for ResetCveListStorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Db(_) => write!(f, "failed to reset CVE List storage"),
            Self::RunningSyncRuns(count) => {
                write!(
                    f,
                    "refusing to reset CVE List storage while {count} sync run(s) are running"
                )
            }
            Self::SyncAlreadyRunning => {
                write!(f, "a CVE List sync run is already running")
            }
            Self::SyncLock(_) => write!(f, "failed to check CVE List sync advisory lock"),
        }
    }
}

impl std::error::Error for ResetCveListStorageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Db(err) => Some(err),
            Self::SyncLock(err) => Some(err),
            Self::RunningSyncRuns(_) | Self::SyncAlreadyRunning => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cve::record::parse_cve_record;
    use camino::Utf8PathBuf;
    use sea_orm::{DbBackend, MockDatabase, MockExecResult, Value};
    use std::collections::BTreeMap;
    use url::Url;

    const TEST_COMMIT_SHA: &str = "0123456789abcdef0123456789abcdef01234567";

    #[test]
    fn write_cve_list_records_returns_zero_counts_for_empty_batches() {
        let db = MockDatabase::new(DbBackend::Postgres).into_connection();

        let summary = run_async(write_cve_list_records(&db, &[])).expect("write should succeed");

        assert_eq!(
            summary,
            WriteCveListRecordsSummary {
                records_seen: 0,
                records_inserted: 0,
                records_updated: 0,
            }
        );
        assert!(db.into_transaction_log().is_empty());
    }

    #[test]
    fn write_cve_list_records_counts_inserts_and_updates() {
        let records = vec![
            parsed_file("CVE-2025-1000"),
            parsed_file("CVE-2026-1000"),
            parsed_file("CVE-2026-1001"),
        ];
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![mock_num_items_row(1)]])
            .append_exec_results(mock_exec_results(1))
            .into_connection();

        let summary =
            run_async(write_cve_list_records(&db, &records)).expect("write should succeed");

        assert_eq!(
            summary,
            WriteCveListRecordsSummary {
                records_seen: 3,
                records_inserted: 2,
                records_updated: 1,
            }
        );

        let transaction_log = db.into_transaction_log();
        assert!(
            transaction_log[0].statements()[0]
                .sql
                .contains("SELECT COUNT")
        );
        assert!(
            transaction_log[1].statements()[0]
                .sql
                .contains("INSERT INTO")
                && transaction_log[1].statements()[0]
                    .sql
                    .contains("cve_list_records")
        );
        assert!(
            transaction_log[1].statements()[0]
                .sql
                .contains(r#"ON CONFLICT ("cve_id") DO UPDATE"#)
        );
        assert!(transaction_log.iter().all(|entry| {
            !entry.statements()[0]
                .sql
                .contains("cve_list_records_staging")
        }));
    }

    #[test]
    fn write_cve_list_records_batches_large_writes() {
        let batch_size = 2;
        let record_count = batch_size + 1;
        let records = (0..record_count)
            .map(|index| parsed_file(&format!("CVE-2026-{}", 1000 + index)))
            .collect::<Vec<_>>();
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![mock_num_items_row(0)], vec![mock_num_items_row(1)]])
            .append_exec_results(mock_exec_results(2))
            .into_connection();

        let summary = run_async(write_cve_list_records_with_batch_size(
            &db, &records, batch_size,
        ))
        .expect("write should succeed");

        assert_eq!(
            summary,
            WriteCveListRecordsSummary {
                records_seen: record_count,
                records_inserted: record_count - 1,
                records_updated: 1,
            }
        );

        let transaction_log = db.into_transaction_log();
        let count_count = transaction_log
            .iter()
            .filter(|entry| entry.statements()[0].sql.contains("SELECT COUNT"))
            .count();
        let upsert_count = transaction_log
            .iter()
            .filter(|entry| {
                entry.statements()[0].sql.contains("INSERT INTO")
                    && entry.statements()[0].sql.contains("cve_list_records")
            })
            .count();

        assert_eq!(count_count, 2);
        assert_eq!(upsert_count, 2);
        assert!(transaction_log.iter().all(|entry| {
            !entry.statements()[0]
                .sql
                .contains("cve_list_records_staging")
        }));
    }

    #[test]
    fn write_cve_list_records_from_receiver_batches_streamed_writes() {
        let batch_size = 2;
        let records = vec![
            parsed_file("CVE-2026-1000"),
            parsed_file("CVE-2026-1001"),
            parsed_file("CVE-2026-1002"),
        ];
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![mock_num_items_row(0)], vec![mock_num_items_row(1)]])
            .append_exec_results(mock_exec_results(2))
            .into_connection();

        let summary = run_async(async {
            let (sender, receiver) = mpsc::channel(records.len());
            for record in records {
                sender.send(record).await.expect("receiver should be open");
            }
            drop(sender);

            write_cve_list_records_from_receiver_with_batch_size_and_progress(
                &db,
                receiver,
                batch_size,
                3,
                &NoopCveListSyncProgress,
            )
            .await
        })
        .expect("write should succeed");

        assert_eq!(
            summary,
            WriteCveListRecordsSummary {
                records_seen: 3,
                records_inserted: 2,
                records_updated: 1,
            }
        );
    }

    #[test]
    fn stage_cve_list_records_from_receiver_batches_streamed_writes() {
        let batch_size = 2;
        let records = vec![
            parsed_file("CVE-2026-1000"),
            parsed_file("CVE-2026-1001"),
            parsed_file("CVE-2026-1002"),
        ];
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![mock_num_items_row(0)], vec![mock_num_items_row(1)]])
            .append_exec_results(mock_exec_results(2))
            .into_connection();

        let summary = run_async(async {
            let (sender, receiver) = mpsc::channel(records.len());
            for record in records {
                sender.send(record).await.expect("receiver should be open");
            }
            drop(sender);

            stage_cve_list_records_from_receiver_with_batch_size_and_progress(
                &db,
                42,
                receiver,
                batch_size,
                3,
                &NoopCveListSyncProgress,
            )
            .await
        })
        .expect("stage should succeed");

        assert_eq!(
            summary,
            WriteCveListRecordsSummary {
                records_seen: 3,
                records_inserted: 2,
                records_updated: 1,
            }
        );

        let transaction_log = db.into_transaction_log();
        let staging_insert_count = transaction_log
            .iter()
            .filter(|entry| {
                entry.statements()[0]
                    .sql
                    .contains(r#"INSERT INTO "public"."cve_list_record_staging""#)
            })
            .count();
        assert_eq!(staging_insert_count, 2);
        assert!(transaction_log.iter().all(|entry| {
            !entry.statements()[0]
                .sql
                .contains(r#"INSERT INTO "public"."cve_list_records""#)
        }));
    }

    #[test]
    fn merge_staged_cve_list_records_upserts_live_records_and_clears_stage() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_exec_results(mock_exec_results(2))
            .into_connection();

        run_async(merge_staged_cve_list_records(&db, 42)).expect("merge should succeed");

        let transaction_log = db.into_transaction_log();
        assert!(transaction_log.iter().any(|entry| {
            entry.statements()[0]
                .sql
                .contains(r#"INSERT INTO "public"."cve_list_records""#)
        }));
        assert!(transaction_log.iter().any(|entry| {
            entry.statements()[0]
                .sql
                .contains(r#"ON CONFLICT ("cve_id") DO UPDATE"#)
        }));
        assert!(transaction_log.iter().any(|entry| {
            entry.statements()[0]
                .sql
                .contains(r#"DELETE FROM "public"."cve_list_record_staging""#)
        }));
    }

    #[test]
    fn insert_new_cve_list_records_from_receiver_batches_streamed_inserts() {
        let batch_size = 2;
        let records = vec![
            parsed_file("CVE-2026-1000"),
            parsed_file("CVE-2026-1001"),
            parsed_file("CVE-2026-1002"),
        ];
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_exec_results(mock_exec_results(2))
            .into_connection();

        let summary = run_async(async {
            let (sender, receiver) = mpsc::channel(records.len());
            for record in records {
                sender.send(record).await.expect("receiver should be open");
            }
            drop(sender);

            insert_new_cve_list_records_from_receiver_with_batch_size_and_progress(
                &db,
                receiver,
                batch_size,
                3,
                &NoopCveListSyncProgress,
            )
            .await
        })
        .expect("write should succeed");

        assert_eq!(
            summary,
            WriteCveListRecordsSummary {
                records_seen: 3,
                records_inserted: 3,
                records_updated: 0,
            }
        );

        let transaction_log = db.into_transaction_log();
        let insert_count = transaction_log
            .iter()
            .filter(|entry| {
                entry.statements()[0]
                    .sql
                    .contains(r#"INSERT INTO "public"."cve_list_records""#)
            })
            .count();
        assert_eq!(insert_count, 2);
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
    fn write_cve_list_records_from_receiver_rejects_duplicate_cve_ids_across_batches() {
        let records = vec![
            parsed_file("CVE-2026-1000"),
            parsed_file("CVE-2026-1001"),
            parsed_file("CVE-2026-1000"),
        ];
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![mock_num_items_row(0)]])
            .append_exec_results(mock_exec_results(1))
            .into_connection();

        let err = run_async(async {
            let (sender, receiver) = mpsc::channel(records.len());
            for record in records {
                sender.send(record).await.expect("receiver should be open");
            }
            drop(sender);

            write_cve_list_records_from_receiver_with_batch_size_and_progress(
                &db,
                receiver,
                2,
                3,
                &NoopCveListSyncProgress,
            )
            .await
        })
        .expect_err("write should fail");

        assert!(matches!(
            err,
            WriteCveListRecordsError::DuplicateCveId(cve_id)
                if cve_id == "CVE-2026-1000"
        ));

        let transaction_log = db.into_transaction_log();
        let upsert_count = transaction_log
            .iter()
            .filter(|entry| {
                entry.statements()[0]
                    .sql
                    .contains(r#"INSERT INTO "public"."cve_list_records""#)
            })
            .count();
        assert_eq!(upsert_count, 1);
    }

    #[test]
    fn write_cve_list_records_rejects_zero_batch_size() {
        let records = vec![parsed_file("CVE-2026-1000")];
        let db = MockDatabase::new(DbBackend::Postgres).into_connection();

        let err = run_async(write_cve_list_records_with_batch_size(&db, &records, 0))
            .expect_err("write should fail");

        assert!(matches!(err, WriteCveListRecordsError::InvalidBatchSize));
        assert!(db.into_transaction_log().is_empty());
    }

    #[test]
    fn write_cve_list_records_upserts_without_replacing_first_seen_at() {
        let records = vec![parsed_file("CVE-2026-1000")];
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![mock_num_items_row(1)]])
            .append_exec_results(mock_exec_results(1))
            .into_connection();

        run_async(write_cve_list_records(&db, &records)).expect("write should succeed");

        let transaction_log = db.into_transaction_log();
        let upsert_sql = &transaction_log[1].statements()[0].sql;
        assert!(upsert_sql.contains("ON CONFLICT"));
        assert!(
            upsert_sql.contains(r#""record_format_version" = "excluded"."record_format_version""#)
        );
        assert!(upsert_sql.contains(r#""record" = "excluded"."record""#));
        assert!(upsert_sql.contains(r#""deleted" = "excluded"."deleted""#));
        assert!(upsert_sql.contains(r#""last_seen_at" = CURRENT_TIMESTAMP"#));
        assert!(upsert_sql.contains(r#""updated_at" = CURRENT_TIMESTAMP"#));
        assert!(!upsert_sql.contains(r#""first_seen_at" = "#));
    }

    #[test]
    fn mark_deleted_cve_list_records_sets_deleted_flag() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_exec_results([MockExecResult {
                last_insert_id: 0,
                rows_affected: 2,
            }])
            .into_connection();

        let summary = run_async(mark_deleted_cve_list_records(
            &db,
            &["CVE-2026-1000".to_owned(), "CVE-2026-1001".to_owned()],
        ))
        .expect("mark deleted should succeed");

        assert_eq!(
            summary,
            WriteCveListRecordsSummary {
                records_seen: 2,
                records_inserted: 0,
                records_updated: 2,
            }
        );

        let transaction_log = db.into_transaction_log();
        let update_sql = &transaction_log[0].statements()[0].sql;
        assert!(update_sql.contains(r#"UPDATE "public"."cve_list_records""#));
        assert!(update_sql.contains(r#""deleted" = $"#));
        assert!(update_sql.contains(r#""updated_at" = CURRENT_TIMESTAMP"#));
        assert!(update_sql.contains(r#""cve_id" IN"#));
    }

    #[test]
    fn mark_cve_list_records_missing_from_staged_snapshot_excludes_staged_ids() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_exec_results([MockExecResult {
                last_insert_id: 0,
                rows_affected: 2,
            }])
            .into_connection();

        let summary = run_async(mark_cve_list_records_missing_from_staged_snapshot(&db, 42))
            .expect("mark missing records should succeed");

        assert_eq!(
            summary,
            WriteCveListRecordsSummary {
                records_seen: 2,
                records_inserted: 0,
                records_updated: 2,
            }
        );

        let transaction_log = db.into_transaction_log();
        let update_sql = &transaction_log[0].statements()[0].sql;
        assert!(update_sql.contains(r#"UPDATE "public"."cve_list_records""#));
        assert!(update_sql.contains(r#""deleted" = $"#));
        assert!(update_sql.contains(r#""updated_at" = CURRENT_TIMESTAMP"#));
        assert!(update_sql.contains(r#""cve_id" NOT IN (SELECT "cve_id""#));
        assert!(update_sql.contains(r#""generation" = $"#));
    }

    #[test]
    fn mark_deleted_cve_list_records_rejects_duplicate_cve_ids() {
        let db = MockDatabase::new(DbBackend::Postgres).into_connection();

        let err = run_async(mark_deleted_cve_list_records(
            &db,
            &["CVE-2026-1000".to_owned(), "CVE-2026-1000".to_owned()],
        ))
        .expect_err("mark deleted should fail");

        assert!(matches!(
            err,
            WriteCveListRecordsError::DuplicateCveId(cve_id)
                if cve_id == "CVE-2026-1000"
        ));
        assert!(db.into_transaction_log().is_empty());
    }

    #[test]
    fn write_cve_list_records_rejects_duplicate_cve_ids() {
        let records = vec![parsed_file("CVE-2026-1000"), parsed_file("CVE-2026-1000")];
        let db = MockDatabase::new(DbBackend::Postgres).into_connection();

        let err = run_async(write_cve_list_records(&db, &records)).expect_err("write should fail");

        assert!(matches!(
            err,
            WriteCveListRecordsError::DuplicateCveId(cve_id)
                if cve_id == "CVE-2026-1000"
        ));
        assert!(db.into_transaction_log().is_empty());
    }

    #[test]
    fn start_cve_list_sync_run_inserts_running_metadata() {
        let repository_url =
            Url::parse("https://github.com/CVEProject/cvelistV5.git").expect("valid URL");
        let repository_ref = GitRef::parse("main").expect("valid Git ref");
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![mock_generation_row(42)]])
            .into_connection();

        let generation = run_async(start_cve_list_sync_run(
            &db,
            &repository_url,
            &repository_ref,
        ))
        .expect("start should succeed");

        assert_eq!(generation, 42);
        let transaction_log = db.into_transaction_log();
        let insert_sql = &transaction_log[0].statements()[0].sql;
        assert!(insert_sql.contains(r#"INSERT INTO "public"."cve_list_sync_runs""#));
        assert!(insert_sql.contains(r#""status""#));
        assert!(insert_sql.contains(r#""repository_url""#));
        assert!(insert_sql.contains(r#""repository_ref""#));
    }

    #[test]
    fn finish_cve_list_sync_run_updates_terminal_metadata() {
        let commit_sha = CommitSha::parse(TEST_COMMIT_SHA).expect("valid commit SHA");
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_exec_results([MockExecResult {
                last_insert_id: 0,
                rows_affected: 1,
            }])
            .into_connection();

        run_async(finish_cve_list_sync_run(
            &db,
            42,
            FinishCveListSyncRun {
                status: CveListSyncRunStatus::Success,
                commit_sha: Some(commit_sha),
                records_seen: 3,
                records_inserted: 2,
                records_updated: 1,
                error: None,
            },
        ))
        .expect("finish should succeed");

        let transaction_log = db.into_transaction_log();
        let update_sql = &transaction_log[0].statements()[0].sql;
        assert!(update_sql.contains(r#"UPDATE "public"."cve_list_sync_runs""#));
        assert!(update_sql.contains(r#""completed_at" = CURRENT_TIMESTAMP"#));
        assert!(update_sql.contains(r#""status" = $"#));
        assert!(update_sql.contains(r#""commit_sha" = $"#));
        assert!(update_sql.contains(r#""records_seen" = $"#));
        assert!(update_sql.contains(r#""records_inserted" = $"#));
        assert!(update_sql.contains(r#""records_updated" = $"#));
        assert!(update_sql.contains(r#""generation" = $"#));
    }

    #[test]
    fn finish_cve_list_sync_run_reports_missing_runs() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_exec_results([MockExecResult {
                last_insert_id: 0,
                rows_affected: 0,
            }])
            .into_connection();

        let err = run_async(finish_cve_list_sync_run(
            &db,
            42,
            FinishCveListSyncRun {
                status: CveListSyncRunStatus::Failed,
                commit_sha: None,
                records_seen: 0,
                records_inserted: 0,
                records_updated: 0,
                error: Some("git failed".to_owned()),
            },
        ))
        .expect_err("finish should fail");

        assert!(matches!(err, CveListSyncRunError::SyncRunNotFound(42)));
    }

    #[test]
    fn finish_cve_list_sync_run_rejects_counts_too_large_for_storage() {
        let db = MockDatabase::new(DbBackend::Postgres).into_connection();
        let too_large = usize::try_from(i32::MAX)
            .expect("i32 max should fit usize")
            .checked_add(1)
            .expect("test count should fit usize");

        let err = run_async(finish_cve_list_sync_run(
            &db,
            42,
            FinishCveListSyncRun {
                status: CveListSyncRunStatus::Success,
                commit_sha: None,
                records_seen: too_large,
                records_inserted: 0,
                records_updated: 0,
                error: None,
            },
        ))
        .expect_err("finish should fail");

        assert!(matches!(
            err,
            CveListSyncRunError::CountTooLarge {
                field: "records_seen",
                count
            } if count == too_large
        ));
        assert!(db.into_transaction_log().is_empty());
    }

    #[test]
    fn finish_cve_list_sync_run_rejects_running_status() {
        let db = MockDatabase::new(DbBackend::Postgres).into_connection();

        let err = run_async(finish_cve_list_sync_run(
            &db,
            42,
            FinishCveListSyncRun {
                status: CveListSyncRunStatus::Running,
                commit_sha: None,
                records_seen: 0,
                records_inserted: 0,
                records_updated: 0,
                error: None,
            },
        ))
        .expect_err("finish should fail");

        assert!(matches!(
            err,
            CveListSyncRunError::NonTerminalStatus(CveListSyncRunStatus::Running)
        ));
        assert!(db.into_transaction_log().is_empty());
    }

    #[test]
    fn last_successful_cve_list_sync_commit_returns_latest_success_commit() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![mock_commit_sha_row(TEST_COMMIT_SHA)]])
            .into_connection();

        let commit = run_async(last_successful_cve_list_sync_commit(&db))
            .expect("lookup should succeed")
            .expect("commit should exist");

        assert_eq!(commit.as_str(), TEST_COMMIT_SHA);
        let transaction_log = db.into_transaction_log();
        let select_sql = &transaction_log[0].statements()[0].sql;
        assert!(select_sql.contains(r#"WHERE "cve_list_sync_runs"."status" = $"#));
        assert!(select_sql.contains(r#"ORDER BY "cve_list_sync_runs"."generation" DESC"#));
        assert!(select_sql.contains("LIMIT $"));
    }

    #[test]
    fn last_successful_cve_list_sync_commit_rejects_invalid_stored_commit_sha() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![mock_commit_sha_row("not-a-commit")]])
            .into_connection();

        let err =
            run_async(last_successful_cve_list_sync_commit(&db)).expect_err("lookup should fail");

        assert!(matches!(
            err,
            CveListSyncRunError::InvalidStoredCommitSha(CveListGitError::InvalidCommitSha(_))
        ));
    }

    #[test]
    fn last_successful_cve_list_sync_commit_for_source_filters_by_source_identity() {
        let repository_url =
            Url::parse("https://github.com/CVEProject/cvelistV5.git").expect("valid URL");
        let repository_ref = GitRef::parse("main").expect("valid Git ref");
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![mock_commit_sha_row(TEST_COMMIT_SHA)]])
            .into_connection();

        let commit = run_async(last_successful_cve_list_sync_commit_for_source(
            &db,
            &repository_url,
            &repository_ref,
        ))
        .expect("lookup should succeed")
        .expect("commit should exist");

        assert_eq!(commit.as_str(), TEST_COMMIT_SHA);
        let transaction_log = db.into_transaction_log();
        let select_sql = &transaction_log[0].statements()[0].sql;
        assert!(select_sql.contains(r#""repository_url" = $"#));
        assert!(select_sql.contains(r#""repository_ref" = $"#));
    }

    #[test]
    fn latest_cve_list_sync_run_returns_latest_run() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![mock_sync_run(42, "success")]])
            .into_connection();

        let run = run_async(latest_cve_list_sync_run(&db))
            .expect("lookup should succeed")
            .expect("run should exist");

        assert_eq!(run.generation, 42);
        assert_eq!(run.status, "success");
        let transaction_log = db.into_transaction_log();
        let select_sql = &transaction_log[0].statements()[0].sql;
        assert!(select_sql.contains(r#"ORDER BY "cve_list_sync_runs"."generation" DESC"#));
        assert!(select_sql.contains("LIMIT $"));
    }

    #[test]
    fn has_cve_list_records_returns_true_when_record_exists() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![mock_cve_id_row("CVE-2026-1000")]])
            .into_connection();

        let has_records = run_async(has_cve_list_records(&db)).expect("lookup should succeed");

        assert!(has_records);
        let transaction_log = db.into_transaction_log();
        let select_sql = &transaction_log[0].statements()[0].sql;
        assert!(select_sql.contains(r#"SELECT "cve_list_records"."cve_id""#));
        assert!(select_sql.contains(r#""cve_list_records"."deleted" = $"#));
        assert!(select_sql.contains("LIMIT $"));
    }

    #[test]
    fn has_cve_list_records_returns_false_for_deleted_only_storage() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([Vec::<BTreeMap<String, Value>>::new()])
            .into_connection();

        let has_records = run_async(has_cve_list_records(&db)).expect("lookup should succeed");

        assert!(!has_records);
        let transaction_log = db.into_transaction_log();
        let select_sql = &transaction_log[0].statements()[0].sql;
        assert!(select_sql.contains(r#""cve_list_records"."deleted" = $"#));
    }

    #[test]
    fn has_cve_list_records_returns_false_when_storage_is_empty() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([Vec::<BTreeMap<String, Value>>::new()])
            .into_connection();

        let has_records = run_async(has_cve_list_records(&db)).expect("lookup should succeed");

        assert!(!has_records);
    }

    #[test]
    fn reset_cve_list_storage_truncates_records_and_sync_runs() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([
                vec![mock_advisory_lock_row(true)],
                vec![mock_num_items_row(0)],
                vec![mock_num_items_row(7)],
                vec![mock_num_items_row(3)],
            ])
            .append_exec_results([MockExecResult {
                last_insert_id: 0,
                rows_affected: 0,
            }])
            .into_connection();

        let summary = run_async(reset_cve_list_storage(&db, false)).expect("reset should succeed");

        assert_eq!(
            summary,
            ResetCveListStorageSummary {
                records_deleted: 7,
                sync_runs_deleted: 3,
            }
        );

        let transaction_log = db.into_transaction_log();
        assert!(
            transaction_log
                .iter()
                .any(|entry| entry.statements()[0].sql.contains(
                    "TRUNCATE TABLE public.cve_list_records, public.cve_list_record_staging, \
                     public.cve_list_sync_runs \
                     RESTART IDENTITY"
                ))
        );
    }

    #[test]
    fn reset_cve_list_storage_rejects_when_sync_lock_is_held() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![mock_advisory_lock_row(false)]])
            .into_connection();

        let err = run_async(reset_cve_list_storage(&db, false)).expect_err("reset should fail");

        assert!(matches!(err, ResetCveListStorageError::SyncAlreadyRunning));
        let transaction_log = db.into_transaction_log();
        assert!(
            transaction_log[0].statements()[0]
                .sql
                .contains("pg_try_advisory_xact_lock")
        );
        assert!(
            transaction_log
                .iter()
                .all(|entry| !entry.statements()[0].sql.contains("TRUNCATE TABLE"))
        );
    }

    #[test]
    fn reset_cve_list_storage_rejects_running_sync_runs_without_force() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([
                vec![mock_advisory_lock_row(true)],
                vec![mock_num_items_row(2)],
            ])
            .into_connection();

        let err = run_async(reset_cve_list_storage(&db, false)).expect_err("reset should fail");

        assert!(matches!(err, ResetCveListStorageError::RunningSyncRuns(2)));
        let transaction_log = db.into_transaction_log();
        assert!(
            transaction_log
                .iter()
                .all(|entry| !entry.statements()[0].sql.contains("TRUNCATE TABLE"))
        );
    }

    #[test]
    fn reset_cve_list_storage_accepts_running_sync_runs_with_force() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([
                vec![mock_advisory_lock_row(true)],
                vec![mock_num_items_row(2)],
                vec![mock_num_items_row(7)],
                vec![mock_num_items_row(3)],
            ])
            .append_exec_results([MockExecResult {
                last_insert_id: 0,
                rows_affected: 0,
            }])
            .into_connection();

        let summary = run_async(reset_cve_list_storage(&db, true)).expect("reset should succeed");

        assert_eq!(
            summary,
            ResetCveListStorageSummary {
                records_deleted: 7,
                sync_runs_deleted: 3,
            }
        );
    }

    #[test]
    fn fail_running_cve_list_sync_runs_marks_running_runs_failed() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_exec_results([MockExecResult {
                last_insert_id: 0,
                rows_affected: 2,
            }])
            .into_connection();

        let summary = run_async(fail_running_cve_list_sync_runs(
            &db,
            "sync process was interrupted",
        ))
        .expect("running sync runs should be marked failed");

        assert_eq!(
            summary,
            FailRunningCveListSyncRunsSummary {
                sync_runs_failed: 2,
            }
        );
        let transaction_log = db.into_transaction_log();
        let update_sql = &transaction_log[0].statements()[0].sql;
        assert!(update_sql.contains(r#"UPDATE "public"."cve_list_sync_runs""#));
        assert!(update_sql.contains(r#""completed_at" = CURRENT_TIMESTAMP"#));
        assert!(update_sql.contains(r#""status" = $"#));
        assert!(update_sql.contains(r#""error" = $"#));
        assert!(update_sql.contains(r#"WHERE "cve_list_sync_runs"."status" = $"#));
    }

    fn mock_num_items_row(num_items: i64) -> BTreeMap<String, Value> {
        BTreeMap::from([("num_items".to_owned(), num_items.into())])
    }

    fn mock_advisory_lock_row(acquired: bool) -> BTreeMap<String, Value> {
        BTreeMap::from([("pg_try_advisory_xact_lock".to_owned(), acquired.into())])
    }

    fn mock_exec_results(count: usize) -> Vec<MockExecResult> {
        (0..count)
            .map(|_| MockExecResult {
                last_insert_id: 0,
                rows_affected: 0,
            })
            .collect()
    }

    fn mock_generation_row(generation: i64) -> BTreeMap<String, Value> {
        BTreeMap::from([("generation".to_owned(), generation.into())])
    }

    fn mock_commit_sha_row(commit_sha: &str) -> BTreeMap<String, Value> {
        BTreeMap::from([("commit_sha".to_owned(), commit_sha.to_owned().into())])
    }

    fn mock_cve_id_row(cve_id: &str) -> BTreeMap<String, Value> {
        BTreeMap::from([("cve_id".to_owned(), cve_id.to_owned().into())])
    }

    fn mock_sync_run(generation: i64, status: &str) -> cve_list_sync_runs::Model {
        cve_list_sync_runs::Model {
            generation,
            checked_at: "2026-07-10T00:00:00Z".parse().expect("valid timestamp"),
            completed_at: Some("2026-07-10T00:01:00Z".parse().expect("valid timestamp")),
            status: status.to_owned(),
            repository_url: Some("https://example.test/cvelistV5.git".to_owned()),
            repository_ref: Some("main".to_owned()),
            commit_sha: Some(TEST_COMMIT_SHA.to_owned()),
            records_seen: 10,
            records_inserted: 7,
            records_updated: 3,
            error: None,
        }
    }

    fn parsed_file(cve_id: &str) -> ParsedCveListFile {
        let record = parse_cve_record(cve_record(cve_id).as_bytes()).expect("record should parse");

        ParsedCveListFile {
            path: Utf8PathBuf::from(format!("cves/2026/1xxx/{cve_id}.json")),
            record,
        }
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
