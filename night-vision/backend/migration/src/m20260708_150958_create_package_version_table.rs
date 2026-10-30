use crate::m20260623_161612_initial_schema::PackageSources;
use crate::m20260630_173030_create_package_table::Packages;
use sea_orm_migration::{
	prelude::*,
	schema::{integer, pk_auto, string, string_null},
};
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
	async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
		manager
			.create_table(
				Table::create()
					.table(PackageVersion::Table)
					.if_not_exists()
					.col(pk_auto(PackageVersion::Id))
					.col(integer(PackageVersion::PackageId))
					.col(integer(PackageVersion::SourceId))
					.col(string(PackageVersion::Version))
					.col(string(PackageVersion::PackageURL))
					.col(string_null(PackageVersion::SourceRepository))
					.col(string_null(PackageVersion::SourceRepositoryTag))
					.foreign_key(
						ForeignKey::create()
							.from(PackageVersion::Table, PackageVersion::PackageId)
							.to(Packages::Table, Packages::Id),
					)
					.foreign_key(
						ForeignKey::create()
							.from(PackageVersion::Table, PackageVersion::SourceId)
							.to(PackageSources::Table, PackageSources::Id),
					)
					.to_owned(),
			)
			.await
	}
	async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
		manager
			.drop_table(Table::drop().table(PackageVersion::Table).to_owned())
			.await
	}
}

#[derive(DeriveIden)]
pub enum PackageVersion {
	Table,
	Id,
	PackageId,
	SourceId,
	Version,
	PackageURL,
	SourceRepository,
	SourceRepositoryTag,
}
