//! Integration tests for the package-source automatic-retry schema: the
//! `attempt_generation`-fenced claim/failure round trip, lease recovery, and
//! the `package_sources_failure_kind`/`package_sources_retryable` CHECK
//! constraints added by `m20260921_000000_package_source_automatic_retry`.

use super::{
    lifecycle::{AttemptKind, FailureKind, begin_attempt, claim_next_due, recover_expired_leases},
    storage::record_elaboration_failure,
};
use crate::{config::Config, db::connection, db::entities::package_sources};
use camino::Utf8PathBuf;
use sea_orm::{
    ActiveModelTrait as _, ActiveValue::Set, ConnectionTrait as _, DatabaseConnection,
    EntityTrait as _,
};
use std::time::Duration;

const CONFIG_PATH_ENV: &str = "NV_POSTGRES_INTEGRATION_CONFIG_PATH";
const DEFAULT_CONFIG_PATH: &str = "src/cve/testdata/nv-server.integration.spookey";

#[test]
#[ignore = "requires a disposable Postgres test database"]
fn automatic_retry_and_lease_recovery_round_trip_through_postgres() {
    run_async(async {
        let db = integration_database().await;
        let source = insert_source(&db, "{}").await;

        // Attempt 1: an automatic claim fails with a retryable kind and is
        // rescheduled rather than left terminal.
        begin_attempt(&db, source.id, AttemptKind::Initial, Duration::from_mins(1))
            .await
            .expect("attempt 1 claims")
            .expect("source is eligible");
        record_elaboration_failure(&db, source.id, 1, FailureKind::DependencyUnavailable)
            .await
            .expect("attempt 1 failure is recorded");
        let after_first_failure = reload(&db, source.id).await;
        assert_eq!(after_first_failure.resolution_status, "pending");
        assert_eq!(after_first_failure.automatic_attempt_count, 1);
        assert!(after_first_failure.retryable);
        assert_eq!(
            after_first_failure.failure_kind.as_deref(),
            Some("dependency-unavailable")
        );

        // Attempt 2: claimed, but the worker "crashes" before finalizing.
        // Forcing its lease into the past and running recovery must treat it
        // as an internal failure and still reschedule it (still under budget).
        begin_attempt(&db, source.id, AttemptKind::Initial, Duration::from_mins(1))
            .await
            .expect("attempt 2 claims")
            .expect("source is due again");
        expire_lease(&db, source.id).await;
        recover_expired_leases(&db, 32, &slog::Logger::root(slog::Discard, slog::o!()))
            .await
            .expect("lease recovery accesses durable state");
        let after_recovery = reload(&db, source.id).await;
        assert_eq!(after_recovery.resolution_status, "pending");
        assert_eq!(after_recovery.automatic_attempt_count, 2);
        assert_eq!(after_recovery.failure_kind.as_deref(), Some("internal"));
        assert!(after_recovery.lease_expires_at.is_none());

        // Attempt 3: exhausts the automatic-attempt budget and stays failed.
        begin_attempt(&db, source.id, AttemptKind::Initial, Duration::from_mins(1))
            .await
            .expect("attempt 3 claims")
            .expect("source is due a third time");
        record_elaboration_failure(&db, source.id, 3, FailureKind::Internal)
            .await
            .expect("attempt 3 failure is recorded");
        let exhausted = reload(&db, source.id).await;
        assert_eq!(exhausted.resolution_status, "failed");
        assert_eq!(exhausted.automatic_attempt_count, 3);
        assert_eq!(exhausted.attempt_generation, 3);
        assert!(
            exhausted.retryable,
            "internal failures remain retryable in kind"
        );
        assert!(exhausted.terminal_at.is_some());

        // A due, exhausted-but-failed source is not picked up automatically.
        assert!(
            claim_next_due(&db, Duration::from_mins(1))
                .await
                .unwrap()
                .is_none()
        );

        // A manual attempt resets the budget: a fresh failure gets three more
        // automatic attempts rather than being immediately terminal.
        begin_attempt(&db, source.id, AttemptKind::Manual, Duration::from_mins(1))
            .await
            .expect("manual attempt claims")
            .expect("failed sources are manually retryable");
        record_elaboration_failure(&db, source.id, 4, FailureKind::DependencyUnavailable)
            .await
            .expect("post-manual failure is recorded");
        let after_manual_retry = reload(&db, source.id).await;
        assert_eq!(after_manual_retry.resolution_status, "pending");
        assert_eq!(
            after_manual_retry.automatic_attempt_count, 0,
            "the manual attempt did not consume any of the automatic-attempt budget"
        );
    });
}

#[test]
#[ignore = "requires a disposable Postgres test database"]
fn database_rejects_invalid_failure_details() {
    run_async(async {
        let db = integration_database().await;
        let source = insert_source(&db, "{}").await;
        begin_attempt(&db, source.id, AttemptKind::Initial, Duration::from_mins(1))
            .await
            .unwrap()
            .unwrap();

        let unrecognized_kind = execute(
            &db,
            &format!(
                "UPDATE package_sources SET failure_kind = 'not-a-real-kind' WHERE id = {}",
                source.id
            ),
        )
        .await;
        assert!(
            unrecognized_kind
                .unwrap_err()
                .to_string()
                .contains("package_sources_failure_kind"),
        );

        let retryable_without_a_retryable_kind = execute(
            &db,
            &format!(
                "UPDATE package_sources SET failure_kind = 'validation', retryable = true WHERE id = {}",
                source.id
            ),
        )
        .await;
        assert!(
            retryable_without_a_retryable_kind
                .unwrap_err()
                .to_string()
                .contains("package_sources_retryable"),
        );
    });
}

#[test]
#[ignore = "requires a disposable Postgres test database"]
fn legacy_backfill_migration_fixes_stuck_processing_and_unclassified_failed_rows() {
    use migration::{
        MigrationTrait as _, SchemaManager, m20260922_000000_package_source_legacy_backfill,
    };

    run_async(async {
        let db = integration_database().await;

        // A `processing` row from before `lease_expires_at` existed: no
        // lease, so `recover_expired_leases` can never reclaim it.
        let stuck_processing = insert_source(&db, "{}").await;
        begin_attempt(
            &db,
            stuck_processing.id,
            AttemptKind::Initial,
            Duration::from_mins(1),
        )
        .await
        .unwrap()
        .unwrap();
        execute(
            &db,
            &format!(
                "UPDATE package_sources SET lease_expires_at = NULL WHERE id = {}",
                stuck_processing.id
            ),
        )
        .await
        .unwrap();

        // A `failed` row from before `failure_kind` existed: unclassifiable,
        // so the status endpoint would fail closed with a 500.
        let unclassified_failed = insert_source(&db, "{}").await;
        begin_attempt(
            &db,
            unclassified_failed.id,
            AttemptKind::Initial,
            Duration::from_mins(1),
        )
        .await
        .unwrap()
        .unwrap();
        record_elaboration_failure(&db, unclassified_failed.id, 1, FailureKind::Internal)
            .await
            .unwrap();
        execute(
            &db,
            &format!(
                "UPDATE package_sources SET failure_kind = NULL, resolution_error = 'raw legacy text', retryable = false WHERE id = {}",
                unclassified_failed.id
            ),
        )
        .await
        .unwrap();

        m20260922_000000_package_source_legacy_backfill::Migration
            .up(&SchemaManager::new(&db))
            .await
            .expect("legacy backfill re-applies cleanly to already-migrated rows");

        let recovered = reload(&db, stuck_processing.id).await;
        assert_eq!(recovered.resolution_status, "pending");
        assert!(recovered.lease_expires_at.is_none());

        let reclassified = reload(&db, unclassified_failed.id).await;
        assert_eq!(reclassified.resolution_status, "failed");
        assert_eq!(reclassified.failure_kind.as_deref(), Some("internal"));
        assert_eq!(
            reclassified.resolution_error.as_deref(),
            Some("Package-source processing failed.")
        );
        assert!(!reclassified.retryable);

        // Rows already in the current shape are untouched (the backfill's
        // WHERE clauses only target rows with legacy-null columns).
        let current = insert_source(&db, "{}").await;
        m20260922_000000_package_source_legacy_backfill::Migration
            .up(&SchemaManager::new(&db))
            .await
            .expect("backfill is a no-op for already-current rows");
        let untouched = reload(&db, current.id).await;
        assert_eq!(untouched.resolution_status, "pending");
    });
}

async fn insert_source(db: &DatabaseConnection, contents: &str) -> package_sources::Model {
    package_sources::ActiveModel {
        source_id: Set(uuid::Uuid::now_v7().to_string()),
        file_name: Set("package.json".to_owned()),
        file_contents: Set(contents.to_owned()),
        inferred_type: Set("npm-package-json".to_owned()),
        resolution_status: Set("pending".to_owned()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("test source inserts")
}

async fn reload(db: &DatabaseConnection, id: i32) -> package_sources::Model {
    package_sources::Entity::find_by_id(id)
        .one(db)
        .await
        .expect("reload query")
        .expect("source still exists")
}

async fn expire_lease(db: &DatabaseConnection, id: i32) {
    execute(
        db,
        &format!(
            "UPDATE package_sources SET lease_expires_at = clock_timestamp() - interval '1 second' WHERE id = {id}"
        ),
    )
    .await
    .expect("test setup SQL");
}

async fn execute(db: &DatabaseConnection, sql: &str) -> Result<(), sea_orm::DbErr> {
    db.execute_unprepared(sql).await.map(|_| ())
}

async fn integration_database() -> DatabaseConnection {
    let config_path = std::env::var(CONFIG_PATH_ENV).map_or_else(
        |_| Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(DEFAULT_CONFIG_PATH),
        Utf8PathBuf::from,
    );
    let config = Config::parse(&config_path).expect("integration configuration");
    connection(&config)
        .await
        .expect("disposable integration database should connect and migrate")
}

fn run_async<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime")
        .block_on(future)
}
