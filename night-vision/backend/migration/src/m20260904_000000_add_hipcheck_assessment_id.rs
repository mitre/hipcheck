use sea_orm_migration::{prelude::*, schema::string};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(HipcheckRuns::Table)
                    .add_column(ColumnDef::new(HipcheckRuns::AssessmentId).string().null())
                    .to_owned(),
            )
            .await?;
        manager
            .get_connection()
            .execute_unprepared(
                "UPDATE hipcheck_runs SET assessment_id = uuidv7()::text WHERE assessment_id IS NULL",
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(HipcheckRuns::Table)
                    .modify_column(string(HipcheckRuns::AssessmentId).not_null())
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_hipcheck_runs_assessment_id")
                    .table(HipcheckRuns::Table)
                    .col(HipcheckRuns::AssessmentId)
                    .unique()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_hipcheck_runs_assessment_id")
                    .table(HipcheckRuns::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(HipcheckRuns::Table)
                    .drop_column(HipcheckRuns::AssessmentId)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum HipcheckRuns {
    Table,
    AssessmentId,
}
