use sea_orm_migration::prelude::*;
#[derive(DeriveMigrationName)]
pub struct Migration;
#[async_trait::async_trait]
impl MigrationTrait for Migration {
	async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
		manager
			.alter_table(
				Table::alter()
					.table(CisaKevEntries::Table)
					.add_column(
						ColumnDef::new(CisaKevEntries::RemovedAt).timestamp_with_time_zone(),
					)
					.to_owned(),
			)
			.await
	}
	async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
		manager
			.alter_table(
				Table::alter()
					.table(CisaKevEntries::Table)
					.drop_column(CisaKevEntries::RemovedAt)
					.to_owned(),
			)
			.await
	}
}
#[derive(DeriveIden)]
enum CisaKevEntries {
	Table,
	RemovedAt,
}
