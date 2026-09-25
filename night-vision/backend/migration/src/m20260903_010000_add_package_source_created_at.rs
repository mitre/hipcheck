use crate::m20260623_161612_initial_schema::PackageSources as ExistingPackageSources;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(ExistingPackageSources::Table)
                    .add_column(
                        ColumnDef::new(PackageSourceColumns::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(ExistingPackageSources::Table)
                    .drop_column(PackageSourceColumns::CreatedAt)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum PackageSourceColumns {
    CreatedAt,
}
