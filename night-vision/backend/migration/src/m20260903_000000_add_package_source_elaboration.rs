use crate::m20260623_161612_initial_schema::PackageSources as ExistingPackageSources;
use crate::m20260630_173030_create_package_table::Packages;
use crate::m20260708_150958_create_package_version_table::PackageVersion;
use sea_orm_migration::{
	prelude::*,
	schema::{integer, integer_null, pk_auto, string, string_null, text_null},
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
	async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
		manager
			.alter_table(
				Table::alter()
					.table(ExistingPackageSources::Table)
					.add_column(string(PackageSources::ResolutionStatus).default("pending"))
					.add_column(text_null(PackageSources::ResolutionError))
					.to_owned(),
			)
			.await?;
		manager
			.create_index(
				Index::create()
					.name("packages_name_host_unique")
					.table(Packages::Table)
					.col(Packages::Name)
					.col(Packages::PackageHost)
					.unique()
					.to_owned(),
			)
			.await?;
		manager
			.create_index(
				Index::create()
					.name("package_version_source_package_version_unique")
					.table(PackageVersion::Table)
					.col(PackageVersion::SourceId)
					.col(PackageVersion::PackageId)
					.col(PackageVersion::Version)
					.unique()
					.to_owned(),
			)
			.await?;
		manager
			.create_table(
				Table::create()
					.table(PackageSourceEdges::Table)
					.if_not_exists()
					.col(pk_auto(PackageSourceEdges::Id))
					.col(integer(PackageSourceEdges::SourceId))
					.col(integer_null(PackageSourceEdges::ParentPackageVersionId))
					.col(integer(PackageSourceEdges::ChildPackageVersionId))
					.col(string_null(PackageSourceEdges::RootDependencyKind))
					.col(string_null(PackageSourceEdges::DeclaredDependency))
					.col(string_null(PackageSourceEdges::DeclaredSpecification))
					.foreign_key(
						ForeignKey::create()
							.from(PackageSourceEdges::Table, PackageSourceEdges::SourceId)
							.to(ExistingPackageSources::Table, ExistingPackageSources::Id),
					)
					.foreign_key(
						ForeignKey::create()
							.from(
								PackageSourceEdges::Table,
								PackageSourceEdges::ParentPackageVersionId,
							)
							.to(PackageVersion::Table, PackageVersion::Id),
					)
					.foreign_key(
						ForeignKey::create()
							.from(
								PackageSourceEdges::Table,
								PackageSourceEdges::ChildPackageVersionId,
							)
							.to(PackageVersion::Table, PackageVersion::Id),
					)
					.to_owned(),
			)
			.await?;
		manager
			.create_table(
				Table::create()
					.table(PackageSourceWarnings::Table)
					.if_not_exists()
					.col(pk_auto(PackageSourceWarnings::Id))
					.col(integer(PackageSourceWarnings::SourceId))
					.col(string_null(PackageSourceWarnings::DeclaredByPurl))
					.col(string(PackageSourceWarnings::DependencyName))
					.col(string(PackageSourceWarnings::SpecificationKind))
					.foreign_key(
						ForeignKey::create()
							.from(
								PackageSourceWarnings::Table,
								PackageSourceWarnings::SourceId,
							)
							.to(ExistingPackageSources::Table, ExistingPackageSources::Id),
					)
					.to_owned(),
			)
			.await
	}

	async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
		manager
			.drop_table(Table::drop().table(PackageSourceWarnings::Table).to_owned())
			.await?;
		manager
			.drop_table(Table::drop().table(PackageSourceEdges::Table).to_owned())
			.await?;
		manager
			.drop_index(
				Index::drop()
					.name("package_version_source_package_version_unique")
					.to_owned(),
			)
			.await?;
		manager
			.drop_index(Index::drop().name("packages_name_host_unique").to_owned())
			.await?;
		manager
			.alter_table(
				Table::alter()
					.table(ExistingPackageSources::Table)
					.drop_column(PackageSources::ResolutionStatus)
					.drop_column(PackageSources::ResolutionError)
					.to_owned(),
			)
			.await
	}
}

#[derive(DeriveIden)]
pub enum PackageSources {
	ResolutionStatus,
	ResolutionError,
}

#[derive(DeriveIden)]
pub enum PackageSourceEdges {
	Table,
	Id,
	SourceId,
	ParentPackageVersionId,
	ChildPackageVersionId,
	RootDependencyKind,
	DeclaredDependency,
	DeclaredSpecification,
}

#[derive(DeriveIden)]
pub enum PackageSourceWarnings {
	Table,
	Id,
	SourceId,
	DeclaredByPurl,
	DependencyName,
	SpecificationKind,
}
