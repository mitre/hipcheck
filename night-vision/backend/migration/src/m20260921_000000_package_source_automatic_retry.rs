//! Durable automatic-retry scheduling and structured failure classification
//! for package sources, layered on the `attempt_generation` fencing and
//! `cancellation_requested`/`terminal_at` columns added by
//! `m20260918_000000_package_source_lifecycle`.

use crate::m20260623_161612_initial_schema::PackageSources;
use sea_orm_migration::{
    prelude::*,
    schema::{boolean, integer, string_null, timestamp_with_time_zone_null},
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(PackageSources::Table)
                    .add_column(
                        ColumnDef::new(PackageSourceColumns::NextAttemptAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .add_column(timestamp_with_time_zone_null(
                        PackageSourceColumns::LeaseExpiresAt,
                    ))
                    .add_column(string_null(PackageSourceColumns::FailureKind))
                    .add_column(boolean(PackageSourceColumns::Retryable).default(false))
                    .add_column(integer(PackageSourceColumns::AutomaticAttemptCount).default(0))
                    .to_owned(),
            )
            .await?;
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE package_sources \
                 ADD CONSTRAINT package_sources_failure_kind CHECK \
                     (failure_kind IS NULL OR failure_kind IN \
                      ('validation', 'dependency-unavailable', 'resolution', 'internal')), \
                 ADD CONSTRAINT package_sources_retryable CHECK \
                     (NOT retryable OR failure_kind IN ('dependency-unavailable', 'internal')), \
                 ADD CONSTRAINT package_sources_automatic_attempt_count CHECK \
                     (automatic_attempt_count >= 0)",
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("package_sources_pending_dispatch")
                    .table(PackageSources::Table)
                    .col(PackageSourceColumns::ResolutionStatus)
                    .col(PackageSourceColumns::NextAttemptAt)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("package_sources_expired_lease")
                    .table(PackageSources::Table)
                    .col(PackageSourceColumns::ResolutionStatus)
                    .col(PackageSourceColumns::LeaseExpiresAt)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("package_sources_expired_lease")
                    .to_owned(),
            )
            .await?;
        manager
            .drop_index(
                Index::drop()
                    .name("package_sources_pending_dispatch")
                    .to_owned(),
            )
            .await?;
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE package_sources \
                 DROP CONSTRAINT package_sources_failure_kind, \
                 DROP CONSTRAINT package_sources_retryable, \
                 DROP CONSTRAINT package_sources_automatic_attempt_count",
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(PackageSources::Table)
                    .drop_column(PackageSourceColumns::NextAttemptAt)
                    .drop_column(PackageSourceColumns::LeaseExpiresAt)
                    .drop_column(PackageSourceColumns::FailureKind)
                    .drop_column(PackageSourceColumns::Retryable)
                    .drop_column(PackageSourceColumns::AutomaticAttemptCount)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum PackageSourceColumns {
    ResolutionStatus,
    NextAttemptAt,
    LeaseExpiresAt,
    FailureKind,
    Retryable,
    AutomaticAttemptCount,
}
