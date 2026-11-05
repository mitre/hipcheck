use sea_orm_migration::{prelude::*, schema::string};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
	async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
		manager
			.create_table(
				Table::create()
					.table(UpgradeAssessments::Table)
					.if_not_exists()
					// UUIDs are stored as canonical strings to keep the API identifier portable
					// across the database tooling already used by this workspace.
					.col(string(UpgradeAssessments::Id).primary_key())
					.col(string(UpgradeAssessments::PackageName))
					.col(string(UpgradeAssessments::CurrentVersion))
					.col(string(UpgradeAssessments::TriggerKind))
					.col(string(UpgradeAssessments::TriggerReference))
					.col(string(UpgradeAssessments::CandidateVersion).null())
					.col(string(UpgradeAssessments::Status))
					.col(
						ColumnDef::new(UpgradeAssessments::CreatedAt)
							.timestamp_with_time_zone()
							.not_null()
							.default(Expr::current_timestamp()),
					)
					.col(ColumnDef::new(UpgradeAssessments::FinishedAt).timestamp_with_time_zone())
					.col(ColumnDef::new(UpgradeAssessments::Report).json_binary())
					.col(ColumnDef::new(UpgradeAssessments::Error).text())
					.check(Expr::cust(
						"status IN ('processing', 'completed', 'failed')",
					))
					.to_owned(),
			)
			.await
	}

	async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
		manager
			.drop_table(Table::drop().table(UpgradeAssessments::Table).to_owned())
			.await
	}
}

#[derive(DeriveIden)]
enum UpgradeAssessments {
	Table,
	Id,
	PackageName,
	CurrentVersion,
	TriggerKind,
	TriggerReference,
	CandidateVersion,
	Status,
	CreatedAt,
	FinishedAt,
	Report,
	Error,
}
