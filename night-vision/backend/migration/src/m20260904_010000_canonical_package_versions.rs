use crate::m20260623_161612_initial_schema::PackageSources;
use crate::m20260708_150958_create_package_version_table::PackageVersion;
use sea_orm_migration::{
    prelude::*,
    schema::{integer, pk_auto},
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(PackageSourceVersions::Table)
                    .if_not_exists()
                    .col(pk_auto(PackageSourceVersions::Id))
                    .col(integer(PackageSourceVersions::SourceId))
                    .col(integer(PackageSourceVersions::PackageVersionId))
                    .foreign_key(
                        ForeignKey::create()
                            .from(
                                PackageSourceVersions::Table,
                                PackageSourceVersions::SourceId,
                            )
                            .to(PackageSources::Table, PackageSources::Id),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(
                                PackageSourceVersions::Table,
                                PackageSourceVersions::PackageVersionId,
                            )
                            .to(PackageVersion::Table, PackageVersion::Id),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("package_source_versions_source_package_version_unique")
                    .table(PackageSourceVersions::Table)
                    .col(PackageSourceVersions::SourceId)
                    .col(PackageSourceVersions::PackageVersionId)
                    .unique()
                    .to_owned(),
            )
            .await?;

        for table in [
            "hipcheck_findings",
            "hipcheck_concerns",
            "hipcheck_checks",
            "hipcheck_runs",
            "package_source_edges",
            "package_source_warnings",
            "package_version",
        ] {
            manager
                .get_connection()
                .execute_unprepared(&format!("DELETE FROM {table}"))
                .await?;
        }
        manager
            .get_connection()
            .execute_unprepared(
                "UPDATE package_sources \
                 SET resolution_status = 'pending', resolution_error = NULL",
            )
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .name("package_version_source_package_version_unique")
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(PackageVersion::Table)
                    .drop_column(PackageVersion::SourceId)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("package_version_package_version_unique")
                    .table(PackageVersion::Table)
                    .col(PackageVersion::PackageId)
                    .col(PackageVersion::Version)
                    .unique()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "canonical package-version migration is irreversible because it resets resolved data"
                .to_owned(),
        ))
    }
}

#[derive(DeriveIden)]
enum PackageSourceVersions {
    Table,
    Id,
    SourceId,
    PackageVersionId,
}
