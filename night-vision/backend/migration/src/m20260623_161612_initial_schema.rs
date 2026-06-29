use sea_orm_migration::{
    prelude::*,
    schema::{pk_auto, string, text},
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(PackageSources::Table)
                    .if_not_exists()
                    .col(pk_auto(PackageSources::Id)) //auto-incrementing primary key column -- id INTEGER PRIMARY KEY
                    .col(string(PackageSources::SourceId))
                    .col(string(PackageSources::FileName))
                    .col(text(PackageSources::FileContents))
                    .col(string(PackageSources::InferredType))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(PackageSources::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum PackageSources {
    Table,
    Id,
    SourceId,
    FileName,
    FileContents,
    InferredType,
}
