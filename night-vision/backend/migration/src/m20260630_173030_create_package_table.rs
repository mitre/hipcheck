use sea_orm_migration::{
	prelude::*,
	schema::{pk_auto, string},
};
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
	async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
		manager
			.create_table(
				Table::create()
					.table(Packages::Table)
					.if_not_exists()
					.col(pk_auto(Packages::Id))
					.col(string(Packages::Name))
					.col(string(Packages::PackageHost))
					.to_owned(),
			)
			.await
	}
	async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
		manager
			.drop_table(Table::drop().table(Packages::Table).to_owned())
			.await
	}
}
#[derive(DeriveIden)]
pub enum Packages {
	Table,
	Id,
	Name,
	PackageHost,
}
