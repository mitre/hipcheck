use sea_orm_migration::{prelude::*, schema::string};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Store CVE List records as source-shaped JSON. The duplicated CVE ID
        // and format version provide stable query fields while JSONB preserves
        // the full upstream record as the CVE Record Format evolves.
        manager
            .create_table(
                Table::create()
                    .table(CveListRecords::Table)
                    .if_not_exists()
                    .col(string(CveListRecords::CveId).primary_key())
                    .col(string(CveListRecords::RecordFormatVersion))
                    .col(
                        ColumnDef::new(CveListRecords::Record)
                            .json_binary()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(CveListRecords::FirstSeenAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(CveListRecords::LastSeenAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(CveListRecords::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .check(Expr::cust("(record->'cveMetadata'->>'cveId') = cve_id"))
                    .check(Expr::cust(
                        "(record->>'dataVersion') = record_format_version",
                    ))
                    .check(Expr::cust("cve_id ~ '^CVE-[0-9]{4}-[0-9]{4,}$'"))
                    .to_owned(),
            )
            .await?;

        // Track each CVE List sync attempt. The upstream data source is a Git
        // repository, so the successful run metadata records the exact commit
        // that produced the imported generation.
        manager
            .create_table(
                Table::create()
                    .table(CveListSyncRuns::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(CveListSyncRuns::Generation)
                            .big_integer()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(CveListSyncRuns::CheckedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(ColumnDef::new(CveListSyncRuns::CompletedAt).timestamp_with_time_zone())
                    .col(string(CveListSyncRuns::Status))
                    .col(string(CveListSyncRuns::RepositoryUrl).null())
                    .col(string(CveListSyncRuns::RepositoryRef).null())
                    .col(string(CveListSyncRuns::CommitSha).null())
                    .col(
                        ColumnDef::new(CveListSyncRuns::RecordsSeen)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(CveListSyncRuns::RecordsInserted)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(CveListSyncRuns::RecordsUpdated)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(ColumnDef::new(CveListSyncRuns::Error).text())
                    .check(Expr::cust(
                        "status IN ('success', 'failed', 'running', 'not_modified')",
                    ))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(CveListSyncRuns::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(CveListRecords::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum CveListRecords {
    Table,
    CveId,
    RecordFormatVersion,
    Record,
    FirstSeenAt,
    LastSeenAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum CveListSyncRuns {
    Table,
    Generation,
    CheckedAt,
    CompletedAt,
    Status,
    RepositoryUrl,
    RepositoryRef,
    CommitSha,
    RecordsSeen,
    RecordsInserted,
    RecordsUpdated,
    Error,
}
