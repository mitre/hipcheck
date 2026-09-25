//! `SeaORM` entity for persisted Hipcheck executions.
use sea_orm::entity::prelude::*;
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "hipcheck_runs")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub assessment_id: String,
    pub package_version_id: i32,
    pub affected_purl: Option<String>,
    pub status: String,
    pub raw_json: Option<String>,
    pub raw_json_bytes: i32,
    pub raw_json_truncated: bool,
    pub stdout: Option<String>,
    pub stdout_truncated: bool,
    pub stderr: Option<String>,
    pub stderr_truncated: bool,
    pub exit_status: Option<i32>,
    pub error_kind: Option<String>,
    pub error_message: Option<String>,
    pub retryable: Option<bool>,
    pub schema_version: Option<String>,
    pub hipcheck_version: Option<String>,
    pub hipcheck_commit: Option<String>,
    pub target_kind: Option<String>,
    pub target_purl: Option<String>,
    pub source_repository_url: Option<String>,
    pub policy_id: Option<String>,
    pub policy_version: Option<String>,
    pub policy_source: Option<String>,
    pub policy_recommendation: Option<String>,
    pub created_at: DateTimeWithTimeZone,
}
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
