use crate::m20260903_000000_add_package_source_elaboration::PackageSourceWarnings;
use sea_orm_migration::{prelude::*, schema::string};

const LEGACY_WARNING_MESSAGE: &str =
	"This dependency specification cannot be resolved through the configured NPM registry.";

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
	async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
		manager
			.alter_table(
				Table::alter()
					.table(PackageSourceWarnings::Table)
					.add_column(
						string(PackageSourceWarningColumns::Message)
							.not_null()
							.default(LEGACY_WARNING_MESSAGE),
					)
					.to_owned(),
			)
			.await
	}

	async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
		manager
			.alter_table(
				Table::alter()
					.table(PackageSourceWarnings::Table)
					.drop_column(PackageSourceWarningColumns::Message)
					.to_owned(),
			)
			.await
	}
}

#[derive(DeriveIden)]
enum PackageSourceWarningColumns {
	Message,
}
