//! Durable package-source lifecycle transitions shared by the API and operator tools.

use crate::{
    db::entities::{
        package_source_deletion_audits, package_source_edges, package_source_versions,
        package_source_warnings, package_sources,
    },
    npm::elaboration::{ElaborationError, PackumentProviderError},
};
use chrono::{DateTime, Duration, Utc};
use sea_orm::{
    ActiveValue::Set,
    ColumnTrait as _, DatabaseConnection, DbErr, EntityTrait as _, QueryFilter as _,
    QueryOrder as _, QuerySelect as _, TransactionTrait as _,
    sea_query::{Expr, OnConflict},
};
use std::time::Duration as StdDuration;
use thiserror::Error;

pub const PACKAGE_SOURCE_RETENTION_DAYS: i64 = 30;
pub const DELETION_AUDIT_RETENTION_DAYS: i64 = 90;
pub const DEFAULT_CLEANUP_BATCH_SIZE: u64 = 100;
pub const MAX_CLEANUP_BATCH_SIZE: u64 = 1_000;

/// Automatic processing is bounded independently of registry-request retries.
pub const MAX_AUTOMATIC_ATTEMPTS: i32 = 3;

/// How many due `pending` rows a single dispatch call inspects before giving up.
const DISPATCH_CANDIDATE_BATCH: u64 = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttemptKind {
    Initial,
    Manual,
}

/// Controlled failure vocabulary. Raw external errors are never durable diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailureKind {
    Validation,
    DependencyUnavailable,
    Resolution,
    Internal,
}

impl FailureKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Validation => "validation",
            Self::DependencyUnavailable => "dependency-unavailable",
            Self::Resolution => "resolution",
            Self::Internal => "internal",
        }
    }

    pub fn from_stored(value: &str) -> Option<Self> {
        match value {
            "validation" => Some(Self::Validation),
            "dependency-unavailable" => Some(Self::DependencyUnavailable),
            "resolution" => Some(Self::Resolution),
            "internal" => Some(Self::Internal),
            _ => None,
        }
    }

    /// A stable, caller-safe diagnostic. Never derived from request data,
    /// dependency errors, or external-tool output.
    pub fn diagnostic(self) -> &'static str {
        match self {
            Self::Validation => "Stored package-source contents are invalid.",
            Self::DependencyUnavailable => {
                "A dependency needed for package-source processing is unavailable."
            }
            Self::Resolution => {
                "Package-source dependencies could not be resolved within the supported limits."
            }
            Self::Internal => "Package-source processing failed.",
        }
    }

    pub fn retryable(self) -> bool {
        matches!(self, Self::DependencyUnavailable | Self::Internal)
    }

    /// Whether a failure at `automatic_attempt_count` (the count covering the
    /// attempt that just failed) should be rescheduled automatically.
    pub fn retries_automatically(self, automatic_attempt_count: i32) -> bool {
        self.retryable() && automatic_attempt_count < MAX_AUTOMATIC_ATTEMPTS
    }
}

impl From<&ElaborationError> for FailureKind {
    fn from(error: &ElaborationError) -> Self {
        match error {
            ElaborationError::InvalidLimits | ElaborationError::WorkerStopped => Self::Internal,
            ElaborationError::Packument { source, .. } => match source {
                PackumentProviderError::Connection
                | PackumentProviderError::Request
                | PackumentProviderError::Timeout
                | PackumentProviderError::ResponseBody => Self::DependencyUnavailable,
                PackumentProviderError::HttpStatus { status }
                    if *status == 408 || *status == 429 || (500..600).contains(status) =>
                {
                    Self::DependencyUnavailable
                }
                _ => Self::Resolution,
            },
            // Repeating deterministic input/size/total-runtime failures cannot
            // make the same immutable source fit the configured limits.
            _ => Self::Resolution,
        }
    }
}

/// Exponential delay plus bounded jitter; callers persist the resulting deadline.
pub fn retry_delay(automatic_attempt_count: i32) -> StdDuration {
    let base_ms: u64 = if automatic_attempt_count <= 1 {
        1_000
    } else {
        2_000
    };
    StdDuration::from_millis(base_ms.saturating_add(fastrand::u64(0..=base_ms)))
}

/// A lease includes publication grace beyond the resolver's total-runtime bound.
pub fn lease_duration(total_run_timeout: StdDuration) -> StdDuration {
    total_run_timeout.saturating_add(StdDuration::from_mins(1))
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

/// Claims a source for processing, fenced by `attempt_generation`.
///
/// `lease` bounds how long the caller may hold the attempt before
/// [`recover_expired_leases`] treats it as abandoned. A `Manual` attempt does
/// not count against the automatic-retry budget: it resets
/// `automatic_attempt_count` to zero, so a failure of this attempt is still
/// eligible for up to [`MAX_AUTOMATIC_ATTEMPTS`] further automatic retries.
pub async fn begin_attempt(
    db: &DatabaseConnection,
    source_id: i32,
    kind: AttemptKind,
    lease: StdDuration,
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
    let automatic_attempt_count = match kind {
        AttemptKind::Initial => source.automatic_attempt_count.saturating_add(1),
        AttemptKind::Manual => 0,
    };
    let lease_expires_at = Utc::now()
        .checked_add_signed(
            Duration::from_std(lease).expect("configured elaboration lease fits a chrono duration"),
        )
        .expect("lease deadline is representable");
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
        .col_expr(
            package_sources::Column::FailureKind,
            Expr::value(None::<String>),
        )
        .col_expr(package_sources::Column::Retryable, Expr::value(false))
        .col_expr(
            package_sources::Column::LeaseExpiresAt,
            Expr::value(Some(lease_expires_at)),
        )
        .col_expr(
            package_sources::Column::AutomaticAttemptCount,
            Expr::value(automatic_attempt_count),
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

/// Claims the earliest due `pending` source for automatic processing,
/// skipping candidates another dispatcher already claimed.
pub async fn claim_next_due(
    db: &DatabaseConnection,
    lease: StdDuration,
) -> Result<Option<package_sources::Model>, PackageSourceLifecycleError> {
    let candidates = package_sources::Entity::find()
        .filter(package_sources::Column::ResolutionStatus.eq("pending"))
        .filter(package_sources::Column::NextAttemptAt.lte(Utc::now()))
        .order_by_asc(package_sources::Column::NextAttemptAt)
        .order_by_asc(package_sources::Column::Id)
        .limit(DISPATCH_CANDIDATE_BATCH)
        .all(db)
        .await
        .map_err(PackageSourceLifecycleError::Database)?;
    for candidate in candidates {
        if begin_attempt(db, candidate.id, AttemptKind::Initial, lease)
            .await?
            .is_some()
        {
            return package_sources::Entity::find_by_id(candidate.id)
                .one(db)
                .await
                .map_err(PackageSourceLifecycleError::Database);
        }
    }
    Ok(None)
}

/// Recovers a bounded batch of processing attempts whose lease has expired.
///
/// Each is treated as an internal failure so the automatic-retry budget
/// still applies. The database clock determines expiry even when server
/// clocks disagree.
///
/// A single row that cannot be finalized is logged and skipped rather than
/// aborting the whole batch: otherwise one bad row would starve recovery of
/// every other expired source.
pub async fn recover_expired_leases(
    db: &DatabaseConnection,
    limit: u64,
    log: &slog::Logger,
) -> Result<(), PackageSourceLifecycleError> {
    let candidates = package_sources::Entity::find()
        .filter(package_sources::Column::ResolutionStatus.eq("processing"))
        .filter(package_sources::Column::LeaseExpiresAt.lte(Utc::now()))
        .order_by_asc(package_sources::Column::LeaseExpiresAt)
        .order_by_asc(package_sources::Column::Id)
        .limit(limit)
        .all(db)
        .await
        .map_err(PackageSourceLifecycleError::Database)?;
    for candidate in candidates {
        if let Err(error) = super::storage::record_elaboration_failure(
            db,
            candidate.id,
            candidate.attempt_generation,
            FailureKind::Internal,
        )
        .await
        {
            slog::warn!(log, "package-source lease recovery could not finalize an expired attempt; leaving it for a later pass";
                "source_id" => candidate.id, "error" => %error);
        }
    }
    Ok(())
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
            display_name: "test package source".to_owned(),
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
            next_attempt_at: Utc::now().fixed_offset(),
            lease_expires_at: (status == "processing").then(|| Utc::now().fixed_offset()),
            failure_kind: None,
            retryable: false,
            automatic_attempt_count: 0,
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
            begin_attempt(&db, 7, AttemptKind::Initial, StdDuration::from_mins(1))
                .await
                .unwrap(),
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
        assert!(
            statements.iter().any(|sql| sql.contains("lease_expires")),
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
            begin_attempt(&db, 7, AttemptKind::Initial, StdDuration::from_mins(1))
                .await
                .unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn manual_attempt_resets_the_automatic_attempt_budget() {
        let mut exhausted = source("failed", Some(Utc::now()));
        exhausted.automatic_attempt_count = MAX_AUTOMATIC_ATTEMPTS;
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![exhausted]])
            .append_exec_results([MockExecResult {
                last_insert_id: 0,
                rows_affected: 1,
            }])
            .into_connection();

        assert_eq!(
            begin_attempt(&db, 7, AttemptKind::Manual, StdDuration::from_mins(1))
                .await
                .unwrap(),
            Some(2)
        );
        let statements = db
            .into_transaction_log()
            .into_iter()
            .flat_map(|entry| entry.statements().to_vec())
            .collect::<Vec<_>>();
        let update = statements
            .iter()
            .find(|statement| {
                statement.sql.starts_with("UPDATE")
                    && statement.sql.contains("automatic_attempt_count")
            })
            .expect("attempt claim resets automatic_attempt_count");
        assert!(
            update
                .values
                .as_ref()
                .expect("update binds values")
                .iter()
                .any(|value| matches!(value, Value::Int(Some(0)))),
            "{update:#?}"
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

    #[test]
    fn only_transient_failures_receive_at_most_three_automatic_attempts() {
        for kind in [FailureKind::Validation, FailureKind::Resolution] {
            assert!(!kind.retryable());
            assert!(!kind.retries_automatically(1));
        }
        for kind in [FailureKind::Internal, FailureKind::DependencyUnavailable] {
            assert!(kind.retries_automatically(1));
            assert!(kind.retries_automatically(2));
            assert!(!kind.retries_automatically(MAX_AUTOMATIC_ATTEMPTS));
            assert!(
                kind.retryable(),
                "exhaustion does not change failure eligibility"
            );
            assert!(kind.retries_automatically(0));
        }
    }

    #[test]
    fn classifies_registry_failures_without_retaining_external_text() {
        use crate::npm::elaboration::{ElaborationError, PackumentProviderError};

        for (status, expected) in [
            (404, FailureKind::Resolution),
            (403, FailureKind::Resolution),
            (429, FailureKind::DependencyUnavailable),
            (503, FailureKind::DependencyUnavailable),
        ] {
            let error = ElaborationError::Packument {
                package: "untrusted-package-contents".into(),
                source: PackumentProviderError::HttpStatus { status },
            };
            let kind = FailureKind::from(&error);
            assert_eq!(kind, expected);
            assert!(!kind.diagnostic().contains("untrusted"));
            assert_eq!(FailureKind::from_stored(kind.as_str()), Some(kind));
        }
        assert_eq!(
            FailureKind::from(&ElaborationError::TotalRunTimeout),
            FailureKind::Resolution
        );
        assert_eq!(
            FailureKind::from(&ElaborationError::WorkerStopped),
            FailureKind::Internal
        );
    }

    #[test]
    fn retry_backoff_is_bounded() {
        for count in [1, 2] {
            let base = StdDuration::from_secs(u64::try_from(count).unwrap());
            let delay = retry_delay(count);
            assert!(delay >= base && delay <= base * 2);
        }
    }

    #[tokio::test]
    async fn claim_next_due_skips_a_candidate_another_dispatcher_already_claimed() {
        let lost_race = source("pending", None);
        let mut won_race = source("pending", None);
        won_race.id = 8;
        let claimed = source("processing", None);
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![lost_race, won_race]])
            .append_query_results([vec![source("pending", None)]])
            .append_exec_results([MockExecResult {
                last_insert_id: 0,
                rows_affected: 0,
            }])
            .append_query_results([vec![source("pending", None)]])
            .append_exec_results([MockExecResult {
                last_insert_id: 0,
                rows_affected: 1,
            }])
            .append_query_results([vec![claimed]])
            .into_connection();

        let claim = claim_next_due(&db, StdDuration::from_mins(1))
            .await
            .unwrap()
            .expect("the second candidate is claimed after the first loses its race");
        assert_eq!(claim.resolution_status, "processing");
    }

    #[tokio::test]
    async fn claim_next_due_finds_nothing_when_no_source_is_due() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([Vec::<package_sources::Model>::new()])
            .into_connection();

        assert!(
            claim_next_due(&db, StdDuration::from_mins(1))
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn recovery_skips_a_row_it_cannot_finalize_and_still_recovers_others() {
        let mut unfinalizable = source("processing", None);
        unfinalizable.id = 1;
        let mut recoverable = source("processing", None);
        recoverable.id = 2;
        let db = MockDatabase::new(DbBackend::Postgres)
            // recover_expired_leases's own expired-lease candidate query.
            .append_query_results([vec![unfinalizable, recoverable.clone()]])
            // record_elaboration_failure's SELECT for the first candidate:
            // no longer an active attempt (lost a race, or was cancelled).
            .append_query_results([Vec::<package_sources::Model>::new()])
            // record_elaboration_failure's SELECT and UPDATE for the second
            // candidate, which finalizes successfully.
            .append_query_results([vec![recoverable]])
            .append_exec_results([MockExecResult {
                last_insert_id: 0,
                rows_affected: 1,
            }])
            .into_connection();

        recover_expired_leases(&db, 32, &slog::Logger::root(slog::Discard, slog::o!()))
            .await
            .expect("one row that cannot be finalized must not abort recovery of the rest");
    }
}
