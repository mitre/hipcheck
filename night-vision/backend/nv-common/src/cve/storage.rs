//! Database storage for parsed CVE List records.

use crate::{
    cve::{
        git::{CommitSha, CveListGitError, GitRef},
        progress::{CveListSyncProgress, CveListSyncProgressReporter, NoopCveListSyncProgress},
        repository::ParsedCveListFile,
    },
    db::entities::cve_list_sync_runs,
};
use sea_orm::{
    ActiveValue::{NotSet, Set},
    ColumnTrait as _, ConnectionTrait, DeriveIden, EntityTrait as _, ExprTrait as _,
    QueryFilter as _, QueryOrder as _, QuerySelect as _,
    sea_query::{Asterisk, ColumnDef, Expr, Func, Iden, JoinType, OnConflict, Query, Table},
};
use std::{collections::HashSet, fmt};
use tokio::sync::mpsc;
use url::Url;

/// Default number of CVE List records to write per database batch.
pub const DEFAULT_CVE_LIST_RECORD_WRITE_BATCH_SIZE: usize = 500;

/// Default number of parsed CVE List records to buffer between parsing and writing.
pub const DEFAULT_CVE_LIST_RECORD_WRITE_CHANNEL_SIZE: usize = 1024;

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
    let mut staging_prepared = false;
    for chunk in records.chunks(batch_size) {
        write_cve_list_record_batch(
            db,
            chunk,
            records.len(),
            &mut summary,
            progress,
            &mut staging_prepared,
        )
        .await?;
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
    let mut staging_prepared = false;

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
            write_cve_list_record_batch(
                db,
                &batch,
                total_records,
                &mut summary,
                progress,
                &mut staging_prepared,
            )
            .await?;
            batch.clear();
        }
    }

    if !batch.is_empty() {
        write_cve_list_record_batch(
            db,
            &batch,
            total_records,
            &mut summary,
            progress,
            &mut staging_prepared,
        )
        .await?;
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

fn count_to_i32(count: usize, field: &'static str) -> Result<i32, CveListSyncRunError> {
    i32::try_from(count).map_err(|_| CveListSyncRunError::CountTooLarge { field, count })
}

fn collect_unique_cve_ids(
    records: &[ParsedCveListFile],
) -> Result<Vec<String>, WriteCveListRecordsError> {
    let mut seen = HashSet::with_capacity(records.len());
    let mut cve_ids = Vec::with_capacity(records.len());

    for record in records {
        let cve_id = record.record.cve_id.as_str().to_owned();
        if !seen.insert(cve_id.clone()) {
            return Err(WriteCveListRecordsError::DuplicateCveId(cve_id));
        }
        cve_ids.push(cve_id);
    }

    Ok(cve_ids)
}

async fn write_cve_list_record_batch<C, P>(
    db: &C,
    records: &[ParsedCveListFile],
    total_records: usize,
    summary: &mut WriteCveListRecordsSummary,
    progress: &P,
    staging_prepared: &mut bool,
) -> Result<(), WriteCveListRecordsError>
where
    C: ConnectionTrait,
    P: CveListSyncProgressReporter + ?Sized,
{
    let cve_ids = collect_unique_cve_ids(records)?;
    if !*staging_prepared {
        prepare_cve_list_record_staging_table(db).await?;
        *staging_prepared = true;
    }
    truncate_cve_list_record_staging_table(db).await?;
    insert_cve_list_record_staging_batch(db, records).await?;

    progress.report(CveListSyncProgress::ExistingRecordLookupStarted {
        records: cve_ids.len(),
    });
    let records_updated = count_existing_staged_cve_records(db).await?;
    progress.report(CveListSyncProgress::ExistingRecordLookupCompleted {
        records: records_updated,
    });

    merge_cve_list_record_staging_table(db).await?;

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

async fn prepare_cve_list_record_staging_table<C>(db: &C) -> Result<(), WriteCveListRecordsError>
where
    C: ConnectionTrait,
{
    let table = Table::create()
        .table(CveListRecordsStaging::Table)
        .temporary()
        .if_not_exists()
        .col(
            ColumnDef::new(CveListRecordsStaging::CveId)
                .text()
                .not_null()
                .primary_key(),
        )
        .col(
            ColumnDef::new(CveListRecordsStaging::RecordFormatVersion)
                .text()
                .not_null(),
        )
        .col(
            ColumnDef::new(CveListRecordsStaging::Record)
                .json_binary()
                .not_null(),
        )
        .extra("ON COMMIT PRESERVE ROWS")
        .take();

    db.execute(&table)
        .await
        .map(|_| ())
        .map_err(WriteCveListRecordsError::Db)
}

async fn truncate_cve_list_record_staging_table<C>(db: &C) -> Result<(), WriteCveListRecordsError>
where
    C: ConnectionTrait,
{
    let table = Table::truncate().table(CveListRecordsStaging::Table).take();

    db.execute(&table)
        .await
        .map(|_| ())
        .map_err(WriteCveListRecordsError::Db)
}

async fn insert_cve_list_record_staging_batch<C>(
    db: &C,
    records: &[ParsedCveListFile],
) -> Result<(), WriteCveListRecordsError>
where
    C: ConnectionTrait,
{
    let mut insert = Query::insert();
    insert.into_table(CveListRecordsStaging::Table).columns([
        CveListRecordsStaging::CveId,
        CveListRecordsStaging::RecordFormatVersion,
        CveListRecordsStaging::Record,
    ]);

    for record in records {
        insert
            .values([
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

async fn count_existing_staged_cve_records<C>(db: &C) -> Result<usize, WriteCveListRecordsError>
where
    C: ConnectionTrait,
{
    let select = Query::select()
        .expr_as(
            Func::count(Expr::col(Asterisk)),
            CveListRecordCountAlias::Existing,
        )
        .from_as(
            CveListRecordsStaging::Table,
            CveListRecordTableAlias::Staging,
        )
        .join_as(
            JoinType::InnerJoin,
            (CveListSchema::Public, CveListRecords::Table),
            CveListRecordTableAlias::Records,
            Expr::col((CveListRecordTableAlias::Records, CveListRecords::CveId)).equals((
                CveListRecordTableAlias::Staging,
                CveListRecordsStaging::CveId,
            )),
        )
        .to_owned();
    let row = db
        .query_one(&select)
        .await
        .map_err(WriteCveListRecordsError::Db)?
        .ok_or_else(|| {
            WriteCveListRecordsError::Db(sea_orm::DbErr::Custom(
                "count query returned no rows".to_owned(),
            ))
        })?;
    let existing_count_alias = CveListRecordCountAlias::Existing.to_string();
    let count = row
        .try_get::<i64>("", existing_count_alias.as_str())
        .map_err(WriteCveListRecordsError::Db)?;

    usize::try_from(count).map_err(|_| WriteCveListRecordsError::NegativeExistingCount(count))
}

async fn merge_cve_list_record_staging_table<C>(db: &C) -> Result<(), WriteCveListRecordsError>
where
    C: ConnectionTrait,
{
    let select = Query::select()
        .columns([
            CveListRecordsStaging::CveId,
            CveListRecordsStaging::RecordFormatVersion,
            CveListRecordsStaging::Record,
        ])
        .from(CveListRecordsStaging::Table)
        .to_owned();
    let mut insert = Query::insert();
    insert
        .into_table((CveListSchema::Public, CveListRecords::Table))
        .columns([
            CveListRecords::CveId,
            CveListRecords::RecordFormatVersion,
            CveListRecords::Record,
        ])
        .select_from(select)
        .map_err(|err| WriteCveListRecordsError::QueryBuild(err.to_string()))?
        .on_conflict(
            OnConflict::column(CveListRecords::CveId)
                .update_columns([CveListRecords::RecordFormatVersion, CveListRecords::Record])
                .values([
                    (CveListRecords::LastSeenAt, Expr::current_timestamp()),
                    (CveListRecords::UpdatedAt, Expr::current_timestamp()),
                ])
                .to_owned(),
        );

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
enum CveListRecords {
    Table,
    CveId,
    RecordFormatVersion,
    Record,
    LastSeenAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum CveListRecordsStaging {
    Table,
    CveId,
    RecordFormatVersion,
    Record,
}

#[derive(DeriveIden)]
enum CveListRecordTableAlias {
    Staging,
    Records,
}

#[derive(DeriveIden)]
enum CveListRecordCountAlias {
    Existing,
}

/// Failure while writing CVE List records to storage.
#[derive(Debug)]
pub enum WriteCveListRecordsError {
    /// A database operation failed.
    Db(sea_orm::DbErr),
    /// The batch contained the same CVE ID more than once.
    DuplicateCveId(String),
    /// The configured write batch size was zero.
    InvalidBatchSize,
    /// The configured parse-to-write channel size was zero.
    InvalidWriteChannelSize,
    /// A staging-table count returned a negative value.
    NegativeExistingCount(i64),
    /// A SeaQuery statement could not be built.
    QueryBuild(String),
}

impl std::fmt::Display for WriteCveListRecordsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
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
            Self::NegativeExistingCount(count) => {
                write!(f, "CVE List staging count was negative: {count}")
            }
            Self::QueryBuild(_) => write!(f, "failed to build CVE List storage query"),
        }
    }
}

impl std::error::Error for WriteCveListRecordsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Db(err) => Some(err),
            Self::DuplicateCveId(_)
            | Self::InvalidBatchSize
            | Self::InvalidWriteChannelSize
            | Self::NegativeExistingCount(_)
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
    /// A non-terminal status was passed when completing a sync run.
    NonTerminalStatus(CveListSyncRunStatus),
    /// The sync run to complete was not found.
    SyncRunNotFound(i64),
}

impl std::fmt::Display for CveListSyncRunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CountTooLarge { field, count } => {
                write!(f, "CVE List sync-run count {field} is too large: {count}")
            }
            Self::Db(_) => write!(f, "failed to update CVE List sync-run metadata"),
            Self::InvalidStoredCommitSha(_) => {
                write!(f, "CVE List sync-run stored an invalid commit SHA")
            }
            Self::NonTerminalStatus(status) => {
                write!(f, "CVE List sync-run status {status} is not terminal")
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
            Self::CountTooLarge { .. } | Self::NonTerminalStatus(_) | Self::SyncRunNotFound(_) => {
                None
            }
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
            .append_query_results([vec![mock_existing_count_row(1)]])
            .append_exec_results(mock_exec_results(4))
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
                .contains(r#"CREATE TEMPORARY TABLE IF NOT EXISTS "cve_list_records_staging""#)
        );
        assert!(
            transaction_log[2].statements()[0]
                .sql
                .contains(r#"INSERT INTO "cve_list_records_staging""#)
        );
        assert!(
            transaction_log[4].statements()[0]
                .sql
                .contains(r#"INSERT INTO "public"."cve_list_records""#)
        );
    }

    #[test]
    fn write_cve_list_records_batches_large_writes() {
        let batch_size = 2;
        let record_count = batch_size + 1;
        let records = (0..record_count)
            .map(|index| parsed_file(&format!("CVE-2026-{}", 1000 + index)))
            .collect::<Vec<_>>();
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([
                vec![mock_existing_count_row(0)],
                vec![mock_existing_count_row(1)],
            ])
            .append_exec_results(mock_exec_results(7))
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
            .filter(|entry| {
                entry.statements()[0]
                    .sql
                    .contains(r#"SELECT COUNT(*) AS "existing""#)
            })
            .count();
        let merge_count = transaction_log
            .iter()
            .filter(|entry| {
                entry.statements()[0]
                    .sql
                    .contains(r#"INSERT INTO "public"."cve_list_records""#)
            })
            .count();
        let staging_insert_count = transaction_log
            .iter()
            .filter(|entry| {
                entry.statements()[0]
                    .sql
                    .contains(r#"INSERT INTO "cve_list_records_staging""#)
            })
            .count();

        assert_eq!(count_count, 2);
        assert_eq!(merge_count, 2);
        assert_eq!(staging_insert_count, 2);
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
            .append_query_results([
                vec![mock_existing_count_row(0)],
                vec![mock_existing_count_row(1)],
            ])
            .append_exec_results(mock_exec_results(7))
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
    fn write_cve_list_records_from_receiver_rejects_duplicate_cve_ids_across_batches() {
        let records = vec![
            parsed_file("CVE-2026-1000"),
            parsed_file("CVE-2026-1001"),
            parsed_file("CVE-2026-1000"),
        ];
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![mock_existing_count_row(0)]])
            .append_exec_results(mock_exec_results(4))
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
        let merge_count = transaction_log
            .iter()
            .filter(|entry| {
                entry.statements()[0]
                    .sql
                    .contains(r#"INSERT INTO "public"."cve_list_records""#)
            })
            .count();
        assert_eq!(merge_count, 1);
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
            .append_query_results([vec![mock_existing_count_row(1)]])
            .append_exec_results(mock_exec_results(4))
            .into_connection();

        run_async(write_cve_list_records(&db, &records)).expect("write should succeed");

        let transaction_log = db.into_transaction_log();
        let merge_sql = &transaction_log[4].statements()[0].sql;
        assert!(merge_sql.contains("ON CONFLICT"));
        assert!(
            merge_sql.contains(r#""record_format_version" = "excluded"."record_format_version""#)
        );
        assert!(merge_sql.contains(r#""record" = "excluded"."record""#));
        assert!(merge_sql.contains(r#""last_seen_at" = CURRENT_TIMESTAMP"#));
        assert!(merge_sql.contains(r#""updated_at" = CURRENT_TIMESTAMP"#));
        assert!(!merge_sql.contains(r#""first_seen_at" = "#));
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

    fn mock_existing_count_row(existing: i64) -> BTreeMap<String, Value> {
        BTreeMap::from([("existing".to_owned(), existing.into())])
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
