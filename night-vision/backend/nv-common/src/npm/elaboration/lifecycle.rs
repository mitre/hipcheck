//! Durable package-source lifecycle transitions shared by the API and operator tools.

use crate::db::entities::{
    package_source_deletion_audits, package_source_edges, package_source_versions,
    package_source_warnings, package_sources,
};
use chrono::{DateTime, Duration, Utc};
use sea_orm::{
    ActiveValue::Set,
    ColumnTrait as _, DatabaseConnection, DbErr, EntityTrait as _, QueryFilter as _,
    QueryOrder as _, QuerySelect as _, TransactionTrait as _,
    sea_query::{Expr, OnConflict},
};
use thiserror::Error;

pub const PACKAGE_SOURCE_RETENTION_DAYS: i64 = 30;
pub const DELETION_AUDIT_RETENTION_DAYS: i64 = 90;
pub const DEFAULT_CLEANUP_BATCH_SIZE: u64 = 100;
pub const MAX_CLEANUP_BATCH_SIZE: u64 = 1_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttemptKind {
    Initial,
    Manual,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CancellationOutcome {
    Accepted,
    Conflict,
    NotFound,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeletionOutcome {
    Accepted,
    NotFound,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CleanupSummary {
    pub scanned: u64,
    pub deleted: u64,
    pub recovered: u64,
    pub skipped: u64,
    pub audits_scanned: u64,
    pub audits_purged: u64,
}

fn increment(value: &mut u64) {
    *value = value
        .checked_add(1)
        .expect("bounded cleanup counter cannot overflow");
}

fn add_len(value: &mut u64, len: usize) {
    let len = u64::try_from(len).expect("collection length must fit in u64");
    *value = value
        .checked_add(len)
        .expect("bounded cleanup counter cannot overflow");
}

#[derive(Debug, Error)]
pub enum PackageSourceLifecycleError {
    #[error("database operation failed")]
    Database(#[source] DbErr),
    #[error("cleanup batch size must be between 1 and {MAX_CLEANUP_BATCH_SIZE}")]
    InvalidBatchSize,
    #[error("package-source attempt generation overflowed")]
    AttemptGenerationOverflow,
    #[error("package-source state changed repeatedly during deletion")]
    ConcurrentTransition,
    #[error("a deleting package source is missing its durable deletion reason")]
    MissingDeletionReason,
}

pub async fn begin_attempt(
    db: &DatabaseConnection,
    source_id: i32,
    kind: AttemptKind,
) -> Result<Option<i32>, PackageSourceLifecycleError> {
    let transaction = db
        .begin()
        .await
        .map_err(PackageSourceLifecycleError::Database)?;
    let Some(source) = package_sources::Entity::find_by_id(source_id)
        .one(&transaction)
        .await
        .map_err(PackageSourceLifecycleError::Database)?
    else {
        return Ok(None);
    };
    let eligible = match kind {
        AttemptKind::Initial => source.resolution_status == "pending",
        AttemptKind::Manual => matches!(
            source.resolution_status.as_str(),
            "pending" | "completed" | "failed"
        ),
    } && !source.cancellation_requested;
    if !eligible {
        return Ok(None);
    }
    let generation = source
        .attempt_generation
        .checked_add(1)
        .ok_or(PackageSourceLifecycleError::AttemptGenerationOverflow)?;
    let updated = package_sources::Entity::update_many()
        .col_expr(
            package_sources::Column::ResolutionStatus,
            Expr::value("processing"),
        )
        .col_expr(
            package_sources::Column::AttemptGeneration,
            Expr::value(generation),
        )
        .col_expr(
            package_sources::Column::CancellationRequested,
            Expr::value(false),
        )
        .col_expr(
            package_sources::Column::TerminalAt,
            Expr::value(None::<DateTime<Utc>>),
        )
        .col_expr(
            package_sources::Column::ResolutionError,
            Expr::value(None::<String>),
        )
        .filter(package_sources::Column::Id.eq(source_id))
        .filter(package_sources::Column::AttemptGeneration.eq(source.attempt_generation))
        .filter(package_sources::Column::ResolutionStatus.eq(source.resolution_status))
        .filter(package_sources::Column::CancellationRequested.eq(false))
        .exec(&transaction)
        .await
        .map_err(PackageSourceLifecycleError::Database)?;
    if updated.rows_affected != 1 {
        return Ok(None);
    }
    transaction
        .commit()
        .await
        .map_err(PackageSourceLifecycleError::Database)?;
    Ok(Some(generation))
}

pub async fn attempt_is_active(
    db: &DatabaseConnection,
    source_id: i32,
    generation: i32,
) -> Result<bool, PackageSourceLifecycleError> {
    package_sources::Entity::find_by_id(source_id)
        .filter(package_sources::Column::ResolutionStatus.eq("processing"))
        .filter(package_sources::Column::AttemptGeneration.eq(generation))
        .filter(package_sources::Column::CancellationRequested.eq(false))
        .one(db)
        .await
        .map(|source| source.is_some())
        .map_err(PackageSourceLifecycleError::Database)
}

pub async fn request_cancellation(
    db: &DatabaseConnection,
    public_id: &str,
    now: DateTime<Utc>,
) -> Result<CancellationOutcome, PackageSourceLifecycleError> {
    let updated = package_sources::Entity::update_many()
        .col_expr(
            package_sources::Column::ResolutionStatus,
            Expr::value("cancelled"),
        )
        .col_expr(
            package_sources::Column::CancellationRequested,
            Expr::value(true),
        )
        .col_expr(package_sources::Column::TerminalAt, Expr::value(Some(now)))
        .filter(package_sources::Column::SourceId.eq(public_id))
        .filter(package_sources::Column::ResolutionStatus.is_in(["pending", "processing"]))
        .exec(db)
        .await
        .map_err(PackageSourceLifecycleError::Database)?;
    if updated.rows_affected == 1 {
        return Ok(CancellationOutcome::Accepted);
    }
    let state = package_sources::Entity::find()
        .filter(package_sources::Column::SourceId.eq(public_id))
        .one(db)
        .await
        .map_err(PackageSourceLifecycleError::Database)?
        .map(|source| source.resolution_status);
    Ok(match state.as_deref() {
        Some("cancelled") => CancellationOutcome::Accepted,
        Some("deleting") | None => CancellationOutcome::NotFound,
        Some(_) => CancellationOutcome::Conflict,
    })
}

pub async fn delete_by_public_id(
    db: &DatabaseConnection,
    public_id: &str,
    reason: &'static str,
) -> Result<DeletionOutcome, PackageSourceLifecycleError> {
    for _ in 0..3 {
        let Some(source) = package_sources::Entity::find()
            .filter(package_sources::Column::SourceId.eq(public_id))
            .one(db)
            .await
            .map_err(PackageSourceLifecycleError::Database)?
        else {
            return Ok(DeletionOutcome::NotFound);
        };
        if source.resolution_status == "deleting"
            || mark_deleting(db, source.id, &source.resolution_status, reason).await?
        {
            finish_deletion(db, source.id).await?;
            return Ok(DeletionOutcome::Accepted);
        }
    }
    // A source can change once from processing to a terminal state while the
    // delete request races publication. A third failed compare-and-set means a
    // concurrent deleter won or removed the row; re-read to preserve 404
    // visibility without guessing.
    let state = package_sources::Entity::find()
        .filter(package_sources::Column::SourceId.eq(public_id))
        .one(db)
        .await
        .map_err(PackageSourceLifecycleError::Database)?;
    match state {
        Some(source) if source.resolution_status == "deleting" => {
            finish_deletion(db, source.id).await?;
            Ok(DeletionOutcome::Accepted)
        }
        Some(_) => Err(PackageSourceLifecycleError::ConcurrentTransition),
        None => Ok(DeletionOutcome::NotFound),
    }
}

async fn mark_deleting(
    db: &DatabaseConnection,
    source_id: i32,
    expected_state: &str,
    reason: &'static str,
) -> Result<bool, PackageSourceLifecycleError> {
    package_sources::Entity::update_many()
        .col_expr(
            package_sources::Column::ResolutionStatus,
            Expr::value("deleting"),
        )
        .col_expr(
            package_sources::Column::CancellationRequested,
            Expr::value(true),
        )
        .col_expr(package_sources::Column::DeletionReason, Expr::value(reason))
        .filter(package_sources::Column::Id.eq(source_id))
        .filter(package_sources::Column::ResolutionStatus.eq(expected_state))
        .exec(db)
        .await
        .map(|result| result.rows_affected == 1)
        .map_err(PackageSourceLifecycleError::Database)
}

async fn finish_deletion(
    db: &DatabaseConnection,
    source_id: i32,
) -> Result<bool, PackageSourceLifecycleError> {
    let transaction = db
        .begin()
        .await
        .map_err(PackageSourceLifecycleError::Database)?;
    let Some(source) = package_sources::Entity::find_by_id(source_id)
        .filter(package_sources::Column::ResolutionStatus.eq("deleting"))
        .one(&transaction)
        .await
        .map_err(PackageSourceLifecycleError::Database)?
    else {
        return Ok(false);
    };
    let reason = source
        .deletion_reason
        .as_deref()
        .ok_or(PackageSourceLifecycleError::MissingDeletionReason)?;
    package_source_edges::Entity::delete_many()
        .filter(package_source_edges::Column::SourceId.eq(source.id))
        .exec(&transaction)
        .await
        .map_err(PackageSourceLifecycleError::Database)?;
    package_source_warnings::Entity::delete_many()
        .filter(package_source_warnings::Column::SourceId.eq(source.id))
        .exec(&transaction)
        .await
        .map_err(PackageSourceLifecycleError::Database)?;
    package_source_versions::Entity::delete_many()
        .filter(package_source_versions::Column::SourceId.eq(source.id))
        .exec(&transaction)
        .await
        .map_err(PackageSourceLifecycleError::Database)?;
    package_source_deletion_audits::Entity::insert(package_source_deletion_audits::ActiveModel {
        source_id: Set(source.source_id),
        reason: Set(reason.to_owned()),
        deleted_at: Set(Utc::now().into()),
        ..Default::default()
    })
    .on_conflict(
        OnConflict::column(package_source_deletion_audits::Column::SourceId)
            .do_nothing()
            .to_owned(),
    )
    .exec(&transaction)
    .await
    .map_err(PackageSourceLifecycleError::Database)?;
    package_sources::Entity::delete_by_id(source.id)
        .exec(&transaction)
        .await
        .map_err(PackageSourceLifecycleError::Database)?;
    transaction
        .commit()
        .await
        .map_err(PackageSourceLifecycleError::Database)?;
    Ok(true)
}

pub async fn cleanup_expired(
    db: &DatabaseConnection,
    now: DateTime<Utc>,
    batch_size: u64,
    dry_run: bool,
) -> Result<CleanupSummary, PackageSourceLifecycleError> {
    if batch_size == 0 || batch_size > MAX_CLEANUP_BATCH_SIZE {
        return Err(PackageSourceLifecycleError::InvalidBatchSize);
    }
    let mut summary = CleanupSummary::default();
    let deleting = package_sources::Entity::find()
        .filter(package_sources::Column::ResolutionStatus.eq("deleting"))
        .order_by_asc(package_sources::Column::Id)
        .limit(batch_size)
        .all(db)
        .await
        .map_err(PackageSourceLifecycleError::Database)?;
    add_len(&mut summary.scanned, deleting.len());
    if !dry_run {
        for source in deleting {
            if finish_deletion(db, source.id).await? {
                increment(&mut summary.deleted);
                increment(&mut summary.recovered);
            } else {
                increment(&mut summary.skipped);
            }
        }
    }
    let remaining = batch_size.saturating_sub(summary.scanned);
    if remaining > 0 {
        let cutoff = now
            .checked_sub_signed(Duration::days(PACKAGE_SOURCE_RETENTION_DAYS))
            .expect("package-source retention duration is representable");
        let expired = package_sources::Entity::find()
            .filter(package_sources::Column::ResolutionStatus.is_in([
                "completed",
                "failed",
                "cancelled",
            ]))
            .filter(package_sources::Column::TerminalAt.lte(cutoff))
            .order_by_asc(package_sources::Column::TerminalAt)
            .order_by_asc(package_sources::Column::Id)
            .limit(remaining)
            .all(db)
            .await
            .map_err(PackageSourceLifecycleError::Database)?;
        add_len(&mut summary.scanned, expired.len());
        if !dry_run {
            for source in expired {
                if mark_deleting(db, source.id, &source.resolution_status, "retention").await?
                    && finish_deletion(db, source.id).await?
                {
                    increment(&mut summary.deleted);
                } else {
                    increment(&mut summary.skipped);
                }
            }
        }
    }
    let audit_cutoff = now
        .checked_sub_signed(Duration::days(DELETION_AUDIT_RETENTION_DAYS))
        .expect("deletion-audit retention duration is representable");
    let expired_audits = package_source_deletion_audits::Entity::find()
        .filter(package_source_deletion_audits::Column::DeletedAt.lte(audit_cutoff))
        .order_by_asc(package_source_deletion_audits::Column::DeletedAt)
        .limit(batch_size)
        .all(db)
        .await
        .map_err(PackageSourceLifecycleError::Database)?;
    summary.audits_scanned =
        u64::try_from(expired_audits.len()).expect("audit batch length must fit in u64");
    if !dry_run && !expired_audits.is_empty() {
        let ids = expired_audits
            .iter()
            .map(|audit| audit.id)
            .collect::<Vec<_>>();
        summary.audits_purged = package_source_deletion_audits::Entity::delete_many()
            .filter(package_source_deletion_audits::Column::Id.is_in(ids))
            .exec(db)
            .await
            .map_err(PackageSourceLifecycleError::Database)?
            .rows_affected;
    }
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::{DbBackend, DbErr, MockDatabase, MockExecResult, Value};

    fn source(status: &str, terminal_at: Option<DateTime<Utc>>) -> package_sources::Model {
        package_sources::Model {
            id: 7,
            source_id: "00000000-0000-7000-8000-000000000007".to_owned(),
            file_name: "package.json".to_owned(),
            file_contents: "{}".to_owned(),
            inferred_type: "npm-package-json".to_owned(),
            resolution_status: status.to_owned(),
            resolution_error: None,
            created_at: Utc::now().fixed_offset(),
            attempt_generation: 1,
            cancellation_requested: status == "cancelled" || status == "deleting",
            terminal_at: terminal_at.map(Into::into),
            deletion_reason: (status == "deleting").then(|| "caller".to_owned()),
        }
    }

    #[tokio::test]
    async fn cancellation_accepts_eligible_work() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_exec_results([MockExecResult {
                last_insert_id: 0,
                rows_affected: 1,
            }])
            .into_connection();

        assert_eq!(
            request_cancellation(&db, "00000000-0000-7000-8000-000000000007", Utc::now(),)
                .await
                .unwrap(),
            CancellationOutcome::Accepted
        );
        let transaction_log = db.into_transaction_log();
        let sql = &transaction_log[0].statements()[0].sql;
        assert!(sql.contains("cancellation_requested"), "{sql}");
        assert!(sql.contains("resolution_status"), "{sql}");
    }

    #[tokio::test]
    async fn cancellation_conflicts_after_publication() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_exec_results([MockExecResult {
                last_insert_id: 0,
                rows_affected: 0,
            }])
            .append_query_results([vec![source("completed", Some(Utc::now()))]])
            .into_connection();

        assert_eq!(
            request_cancellation(&db, "00000000-0000-7000-8000-000000000007", Utc::now(),)
                .await
                .unwrap(),
            CancellationOutcome::Conflict
        );
    }

    #[tokio::test]
    async fn cancellation_is_idempotent_for_cancelled_work() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_exec_results([MockExecResult {
                last_insert_id: 0,
                rows_affected: 0,
            }])
            .append_query_results([vec![source("cancelled", Some(Utc::now()))]])
            .into_connection();

        assert_eq!(
            request_cancellation(&db, "00000000-0000-7000-8000-000000000007", Utc::now())
                .await
                .unwrap(),
            CancellationOutcome::Accepted
        );
    }

    #[tokio::test]
    async fn cancellation_hides_deleting_and_unknown_sources() {
        for query_result in [vec![source("deleting", None)], Vec::new()] {
            let db = MockDatabase::new(DbBackend::Postgres)
                .append_exec_results([MockExecResult {
                    last_insert_id: 0,
                    rows_affected: 0,
                }])
                .append_query_results([query_result])
                .into_connection();

            assert_eq!(
                request_cancellation(&db, "00000000-0000-7000-8000-000000000007", Utc::now(),)
                    .await
                    .unwrap(),
                CancellationOutcome::NotFound
            );
        }
    }

    #[tokio::test]
    async fn cancellation_reports_database_errors_without_changing_the_outcome() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_exec_errors([DbErr::Custom("database unavailable".to_owned())])
            .into_connection();

        assert!(matches!(
            request_cancellation(&db, "00000000-0000-7000-8000-000000000007", Utc::now()).await,
            Err(PackageSourceLifecycleError::Database(_))
        ));
    }

    #[tokio::test]
    async fn attempt_claim_uses_generation_compare_and_set() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![source("pending", None)]])
            .append_exec_results([MockExecResult {
                last_insert_id: 0,
                rows_affected: 1,
            }])
            .into_connection();

        assert_eq!(
            begin_attempt(&db, 7, AttemptKind::Initial).await.unwrap(),
            Some(2)
        );
        let statements = db
            .into_transaction_log()
            .into_iter()
            .flat_map(|entry| entry.statements().to_vec())
            .map(|statement| statement.sql)
            .collect::<Vec<_>>();
        assert!(
            statements
                .iter()
                .any(|sql| sql.contains("attempt_generation")),
            "{statements:#?}"
        );
    }

    #[tokio::test]
    async fn attempt_claim_loses_cleanly_to_a_concurrent_transition() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![source("pending", None)]])
            .append_exec_results([MockExecResult {
                last_insert_id: 0,
                rows_affected: 0,
            }])
            .into_connection();

        assert_eq!(
            begin_attempt(&db, 7, AttemptKind::Initial).await.unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn deletion_removes_only_source_scoped_relationships() {
        let deleting = source("deleting", None);
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![source("processing", None)], vec![deleting]])
            .append_query_results([vec![package_source_deletion_audits::Model {
                id: 1,
                source_id: "00000000-0000-7000-8000-000000000007".to_owned(),
                reason: "caller".to_owned(),
                deleted_at: Utc::now().into(),
            }]])
            .append_exec_results(std::iter::repeat_n(
                MockExecResult {
                    last_insert_id: 1,
                    rows_affected: 1,
                },
                5,
            ))
            .into_connection();

        assert_eq!(
            delete_by_public_id(&db, "00000000-0000-7000-8000-000000000007", "caller",)
                .await
                .unwrap(),
            DeletionOutcome::Accepted
        );
        let statements = db
            .into_transaction_log()
            .into_iter()
            .flat_map(|entry| entry.statements().to_vec())
            .map(|statement| statement.sql)
            .collect::<Vec<_>>();
        assert!(
            statements
                .iter()
                .any(|sql| sql.contains("package_source_edges"))
        );
        assert!(
            statements
                .iter()
                .any(|sql| sql.contains("package_source_warnings"))
        );
        assert!(
            statements
                .iter()
                .any(|sql| sql.contains("package_source_versions"))
        );
        assert!(
            !statements
                .iter()
                .any(|sql| sql.contains("DELETE FROM \"package_versions\""))
        );
        assert!(statements.iter().any(|sql| sql.contains("deletion_reason")));
    }

    #[tokio::test]
    async fn deletion_of_an_unknown_source_is_hidden() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([Vec::<package_sources::Model>::new()])
            .into_connection();

        assert_eq!(
            delete_by_public_id(&db, "00000000-0000-7000-8000-000000000007", "caller")
                .await
                .unwrap(),
            DeletionOutcome::NotFound
        );
    }

    #[tokio::test]
    async fn deletion_reports_repeated_concurrent_transitions() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results(std::iter::repeat_n(vec![source("processing", None)], 4))
            .append_exec_results(std::iter::repeat_n(
                MockExecResult {
                    last_insert_id: 0,
                    rows_affected: 0,
                },
                3,
            ))
            .into_connection();

        assert!(matches!(
            delete_by_public_id(&db, "00000000-0000-7000-8000-000000000007", "caller").await,
            Err(PackageSourceLifecycleError::ConcurrentTransition)
        ));
    }

    #[tokio::test]
    async fn cleanup_dry_run_is_bounded_and_non_mutating() {
        let now = Utc::now();
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([Vec::<package_sources::Model>::new()])
            .append_query_results([vec![source(
                "completed",
                now.checked_sub_signed(Duration::days(31)),
            )]])
            .append_query_results([Vec::<package_source_deletion_audits::Model>::new()])
            .into_connection();

        let summary = cleanup_expired(&db, now, 1, true).await.unwrap();
        assert_eq!(summary.scanned, 1);
        assert_eq!(summary.deleted, 0);
        assert!(db.into_transaction_log().iter().all(|entry| {
            entry
                .statements()
                .iter()
                .all(|statement| !statement.sql.contains("DELETE"))
        }));
    }

    #[tokio::test]
    async fn cleanup_purges_expired_minimal_audits() {
        let now = Utc::now();
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([Vec::<package_sources::Model>::new()])
            .append_query_results([Vec::<package_sources::Model>::new()])
            .append_query_results([vec![package_source_deletion_audits::Model {
                id: 9,
                source_id: "00000000-0000-7000-8000-000000000009".to_owned(),
                reason: "caller".to_owned(),
                deleted_at: now
                    .checked_sub_signed(Duration::days(91))
                    .expect("test timestamp is representable")
                    .into(),
            }]])
            .append_exec_results([MockExecResult {
                last_insert_id: 0,
                rows_affected: 1,
            }])
            .into_connection();

        let summary = cleanup_expired(&db, now, 10, false).await.unwrap();
        assert_eq!(summary.audits_purged, 1);
    }

    #[tokio::test]
    async fn cleanup_deletes_expired_sources_with_a_retention_audit() {
        let now = Utc::now();
        let completed = source("completed", now.checked_sub_signed(Duration::days(31)));
        let mut deleting = completed.clone();
        deleting.resolution_status = "deleting".to_owned();
        deleting.cancellation_requested = true;
        deleting.deletion_reason = Some("retention".to_owned());
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([Vec::<package_sources::Model>::new()])
            .append_query_results([vec![completed], vec![deleting]])
            .append_query_results([
                vec![package_source_deletion_audits::Model {
                    id: 1,
                    source_id: "00000000-0000-7000-8000-000000000007".to_owned(),
                    reason: "retention".to_owned(),
                    deleted_at: now.into(),
                }],
                Vec::new(),
            ])
            .append_exec_results(std::iter::repeat_n(
                MockExecResult {
                    last_insert_id: 1,
                    rows_affected: 1,
                },
                5,
            ))
            .into_connection();

        let summary = cleanup_expired(&db, now, 10, false).await.unwrap();
        assert_eq!(summary.scanned, 1);
        assert_eq!(summary.deleted, 1);
        assert_eq!(summary.recovered, 0);
    }

    #[tokio::test]
    async fn cleanup_recovers_deletion_with_its_original_reason() {
        let now = Utc::now();
        let mut deleting = source("deleting", None);
        deleting.deletion_reason = Some("retention".to_owned());
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![deleting.clone()], vec![deleting]])
            .append_query_results([
                vec![package_source_deletion_audits::Model {
                    id: 1,
                    source_id: "00000000-0000-7000-8000-000000000007".to_owned(),
                    reason: "retention".to_owned(),
                    deleted_at: now.into(),
                }],
                Vec::new(),
            ])
            .append_exec_results(std::iter::repeat_n(
                MockExecResult {
                    last_insert_id: 1,
                    rows_affected: 1,
                },
                4,
            ))
            .into_connection();

        let summary = cleanup_expired(&db, now, 1, false).await.unwrap();
        assert_eq!(summary.deleted, 1);
        assert_eq!(summary.recovered, 1);
        let transaction_log = db.into_transaction_log();
        let audit_insert = transaction_log
            .iter()
            .flat_map(sea_orm::Transaction::statements)
            .find(|statement| statement.sql.contains("package_source_deletion_audits"))
            .expect("recovery inserts a deletion audit");
        let values = audit_insert
            .values
            .as_ref()
            .expect("audit insertion uses bound values");
        assert!(values.iter().any(|value| matches!(
            value,
            Value::String(Some(reason)) if reason == "retention"
        )));
    }

    #[tokio::test]
    async fn cleanup_is_safe_when_repeated_after_all_work_is_done() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([Vec::<package_sources::Model>::new()])
            .append_query_results([Vec::<package_sources::Model>::new()])
            .append_query_results([Vec::<package_source_deletion_audits::Model>::new()])
            .into_connection();

        assert_eq!(
            cleanup_expired(&db, Utc::now(), 100, false).await.unwrap(),
            CleanupSummary::default()
        );
    }

    #[tokio::test]
    async fn cleanup_rejects_unbounded_batches() {
        let db = MockDatabase::new(DbBackend::Postgres).into_connection();
        assert!(matches!(
            cleanup_expired(&db, Utc::now(), 0, false).await,
            Err(PackageSourceLifecycleError::InvalidBatchSize)
        ));
        assert!(matches!(
            cleanup_expired(&db, Utc::now(), MAX_CLEANUP_BATCH_SIZE + 1, false).await,
            Err(PackageSourceLifecycleError::InvalidBatchSize)
        ));
    }
}
