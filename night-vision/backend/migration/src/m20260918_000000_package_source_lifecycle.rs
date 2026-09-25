use crate::m20260623_161612_initial_schema::PackageSources;
use sea_orm_migration::{
	prelude::*,
	schema::{
		boolean, integer, pk_auto, string, string_null, timestamp_with_time_zone,
		timestamp_with_time_zone_null,
	},
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
					.add_column(integer(PackageSourceColumns::AttemptGeneration).default(0))
					.add_column(boolean(PackageSourceColumns::CancellationRequested).default(false))
					.add_column(timestamp_with_time_zone_null(
						PackageSourceColumns::TerminalAt,
					))
					.add_column(string_null(PackageSourceColumns::DeletionReason))
					.to_owned(),
			)
			.await?;
		manager
			.get_connection()
			.execute_unprepared(
				"UPDATE package_sources SET terminal_at = created_at \
                 WHERE resolution_status IN ('completed', 'failed', 'cancelled')",
			)
			.await?;
		manager
			.create_index(
				Index::create()
					.name("package_sources_retention_candidates")
					.table(PackageSources::Table)
					.col(PackageSourceColumns::ResolutionStatus)
					.col(PackageSourceColumns::TerminalAt)
					.to_owned(),
			)
			.await?;
		manager
			.create_table(
				Table::create()
					.table(PackageSourceDeletionAudits::Table)
					.if_not_exists()
					.col(pk_auto(PackageSourceDeletionAudits::Id))
					.col(string(PackageSourceDeletionAudits::SourceId))
					.col(string(PackageSourceDeletionAudits::Reason))
					.col(timestamp_with_time_zone(
						PackageSourceDeletionAudits::DeletedAt,
					))
					.to_owned(),
			)
			.await?;
		manager
			.create_index(
				Index::create()
					.name("package_source_deletion_audits_source_unique")
					.table(PackageSourceDeletionAudits::Table)
					.col(PackageSourceDeletionAudits::SourceId)
					.unique()
					.to_owned(),
			)
			.await?;
		manager
			.create_index(
				Index::create()
					.name("package_source_deletion_audits_retention")
					.table(PackageSourceDeletionAudits::Table)
					.col(PackageSourceDeletionAudits::DeletedAt)
					.to_owned(),
			)
			.await
	}

	async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
		manager
			.drop_table(
				Table::drop()
					.table(PackageSourceDeletionAudits::Table)
					.to_owned(),
			)
			.await?;
		manager
			.drop_index(
				Index::drop()
					.name("package_sources_retention_candidates")
					.to_owned(),
			)
			.await?;
		manager
			.alter_table(
				Table::alter()
					.table(PackageSources::Table)
					.drop_column(PackageSourceColumns::AttemptGeneration)
					.drop_column(PackageSourceColumns::CancellationRequested)
					.drop_column(PackageSourceColumns::TerminalAt)
					.drop_column(PackageSourceColumns::DeletionReason)
					.to_owned(),
			)
			.await
	}
}

#[derive(DeriveIden)]
enum PackageSourceColumns {
	AttemptGeneration,
	CancellationRequested,
	ResolutionStatus,
	TerminalAt,
	DeletionReason,
}

#[derive(DeriveIden)]
enum PackageSourceDeletionAudits {
	Table,
	Id,
	SourceId,
	Reason,
	DeletedAt,
}
