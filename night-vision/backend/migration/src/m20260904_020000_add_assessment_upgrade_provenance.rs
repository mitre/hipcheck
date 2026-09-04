use sea_orm_migration::{prelude::*, schema::string_null};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(HipcheckRuns::Table)
                    .add_column(string_null(HipcheckRuns::AffectedPurl))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(HipcheckRuns::Table)
                    .drop_column(HipcheckRuns::AffectedPurl)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum HipcheckRuns {
    Table,
    AffectedPurl,
}
