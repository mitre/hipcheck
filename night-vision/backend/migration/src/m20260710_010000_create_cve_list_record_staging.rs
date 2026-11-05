use sea_orm_migration::{prelude::*, schema::string};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
	async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
		manager
			.create_table(
				Table::create()
					.table(CveListRecordStaging::Table)
					.if_not_exists()
					.col(ColumnDef::new(CveListRecordStaging::Generation).big_integer())
					.col(string(CveListRecordStaging::CveId))
					.col(string(CveListRecordStaging::RecordFormatVersion))
					.col(
						ColumnDef::new(CveListRecordStaging::Record)
							.json_binary()
							.not_null(),
					)
					.col(
						ColumnDef::new(CveListRecordStaging::CreatedAt)
							.timestamp_with_time_zone()
							.not_null()
							.default(Expr::current_timestamp()),
					)
					.primary_key(
						Index::create()
							.col(CveListRecordStaging::Generation)
							.col(CveListRecordStaging::CveId),
					)
					.foreign_key(
						ForeignKey::create()
							.from(
								CveListRecordStaging::Table,
								CveListRecordStaging::Generation,
							)
							.to(CveListSyncRuns::Table, CveListSyncRuns::Generation)
							.on_delete(ForeignKeyAction::Cascade),
					)
					.check(Expr::cust("(record->'cveMetadata'->>'cveId') = cve_id"))
					.check(Expr::cust(
						"(record->>'dataVersion') = record_format_version",
					))
					.check(Expr::cust("cve_id ~ '^CVE-[0-9]{4}-[0-9]{4,}$'"))
					.to_owned(),
			)
			.await
	}

	async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
		manager
			.drop_table(Table::drop().table(CveListRecordStaging::Table).to_owned())
			.await
	}
}

#[derive(DeriveIden)]
enum CveListRecordStaging {
	Table,
	Generation,
	CveId,
	RecordFormatVersion,
	Record,
	CreatedAt,
}

#[derive(DeriveIden)]
enum CveListSyncRuns {
	Table,
	Generation,
}
