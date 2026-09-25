use sea_orm_migration::{prelude::*, schema::string};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Store KEV vulnerability records as source-shaped JSON instead of
        // projecting each CISA field into a relational column. The duplicated
        // CVE ID gives us a stable natural key for upserts and joins while the
        // JSONB payload preserves the full upstream entry as CISA publishes it.
        manager
            .create_table(
                Table::create()
                    .table(CisaKevEntries::Table)
                    .if_not_exists()
                    .col(string(CisaKevEntries::CveId).primary_key())
                    .col(
                        ColumnDef::new(CisaKevEntries::Entry)
                            .json_binary()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(CisaKevEntries::FirstSeenAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(CisaKevEntries::LastSeenAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(CisaKevEntries::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    // Keep the duplicated natural key tied to the upstream
                    // payload and reject malformed CVE IDs at the boundary.
                    .check(Expr::cust("(entry->>'cveID') = cve_id"))
                    .check(Expr::cust("cve_id ~ '^CVE-[0-9]{4}-[0-9]{4,}$'"))
                    .to_owned(),
            )
            .await?;

        // Track ingestion runs separately from KEV entries. Each poll creates
        // a new generation, giving the application a stable total order for
        // "most recent run" and "most recent successful run" queries while
        // preserving failures and cache metadata for debugging.
        manager
            .create_table(
                Table::create()
                    .table(CisaKevSyncRuns::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(CisaKevSyncRuns::Generation)
                            .big_integer()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(CisaKevSyncRuns::CheckedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(ColumnDef::new(CisaKevSyncRuns::CompletedAt).timestamp_with_time_zone())
                    .col(string(CisaKevSyncRuns::Status))
                    .col(string(CisaKevSyncRuns::CatalogVersion).null())
                    .col(
                        ColumnDef::new(CisaKevSyncRuns::CatalogDateReleased)
                            .timestamp_with_time_zone(),
                    )
                    .col(ColumnDef::new(CisaKevSyncRuns::CatalogCount).integer())
                    .col(string(CisaKevSyncRuns::Etag).null())
                    .col(string(CisaKevSyncRuns::LastModified).null())
                    .col(string(CisaKevSyncRuns::ContentSha256).null())
                    // These counts describe this generation only, not lifetime
                    // totals. They are for observability and import reporting.
                    .col(
                        ColumnDef::new(CisaKevSyncRuns::RecordsSeen)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(CisaKevSyncRuns::RecordsInserted)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(CisaKevSyncRuns::RecordsUpdated)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(ColumnDef::new(CisaKevSyncRuns::Error).text())
                    .check(Expr::cust(
                        "status IN ('success', 'failed', 'running', 'not_modified')",
                    ))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(CisaKevSyncRuns::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(CisaKevEntries::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum CisaKevEntries {
    Table,
    CveId,
    Entry,
    FirstSeenAt,
    LastSeenAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum CisaKevSyncRuns {
    Table,
    Generation,
    CheckedAt,
    CompletedAt,
    Status,
    CatalogVersion,
    CatalogDateReleased,
    CatalogCount,
    Etag,
    LastModified,
    ContentSha256,
    RecordsSeen,
    RecordsInserted,
    RecordsUpdated,
    Error,
}
