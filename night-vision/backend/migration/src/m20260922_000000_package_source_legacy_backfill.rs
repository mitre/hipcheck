//! Backfills two gaps `m20260921_000000_package_source_automatic_retry` left
//! in rows that predate it:
//!
//! - A `processing` row from before `lease_expires_at` existed has no lease,
//!   so `recover_expired_leases`'s `lease_expires_at <= now()` filter never
//!   matches it (`NULL` comparisons are never true) and it can never be
//!   reclaimed. Requeue it as immediately-due pending work; its existing
//!   `attempt_generation` still fences any lingering pre-migration claim.
//! - A `failed` row from before `failure_kind` existed cannot be classified
//!   by `FailureKind::from_stored`, so `GET` fails closed with a `500`.
//!   Replace its diagnostic with the fixed internal-failure representation;
//!   conservatively not retryable, since its original classification and
//!   retry budget were never durable.
//!
//! This is a data-only migration: it adds no columns or constraints.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
	async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
		manager
			.get_connection()
			.execute_unprepared(
				"
UPDATE package_sources
SET resolution_status = 'pending',
    next_attempt_at = clock_timestamp()
WHERE resolution_status = 'processing' AND lease_expires_at IS NULL;

UPDATE package_sources
SET failure_kind = 'internal',
    resolution_error = 'Package-source processing failed.',
    retryable = FALSE
WHERE resolution_status = 'failed' AND failure_kind IS NULL;
",
			)
			.await?;
		Ok(())
	}

	async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
		// This migration only corrects rows that were otherwise stuck or
		// unreadable; there is no prior state worth restoring on rollback.
		Ok(())
	}
}
