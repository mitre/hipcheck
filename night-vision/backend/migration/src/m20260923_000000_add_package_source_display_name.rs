//! Adds package-source display names.

use crate::m20260623_161612_initial_schema::PackageSources;
use sea_orm_migration::{prelude::*, schema::string};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
	async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
		manager
			.alter_table(
				Table::alter()
					.table(PackageSources::Table)
					.add_column(string(PackageSourceColumns::DisplayName).default("package.json"))
					.to_owned(),
			)
			.await?;
		// Backfill stable, distinct labels for legacy sources.
		manager
			.get_connection()
			.execute_unprepared(
				"UPDATE package_sources \
                 SET display_name = 'package.json — ' || substring(source_id from 1 for 8)",
			)
			.await?;
		manager
			.get_connection()
			.execute_unprepared(
				"ALTER TABLE package_sources ALTER COLUMN display_name DROP DEFAULT",
			)
			.await?;
		manager
			.create_index(
				Index::create()
					.name("package_sources_display_name")
					.table(PackageSources::Table)
					.col(PackageSourceColumns::DisplayName)
					.to_owned(),
			)
			.await
	}

	async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
		manager
			.drop_index(
				Index::drop()
					.name("package_sources_display_name")
					.to_owned(),
			)
			.await?;
		manager
			.alter_table(
				Table::alter()
					.table(PackageSources::Table)
					.drop_column(PackageSourceColumns::DisplayName)
					.to_owned(),
			)
			.await
	}
}

#[derive(DeriveIden)]
enum PackageSourceColumns {
	DisplayName,
}
