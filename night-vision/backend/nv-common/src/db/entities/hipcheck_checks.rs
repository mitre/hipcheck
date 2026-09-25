//! `SeaORM` entity for normalized Hipcheck checks.
use sea_orm::entity::prelude::*;
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "hipcheck_checks")]
pub struct Model {
	#[sea_orm(primary_key)]
	pub id: i32,
	pub run_id: i32,
	pub ordinal: i32,
	pub plugin_name: String,
	pub plugin_publisher: String,
	pub plugin_version: String,
	pub plugin_query: String,
	pub policy_expression: String,
	pub state: String,
	pub effect: String,
	pub severity: Option<String>,
	pub summary: String,
	pub value: Json,
	pub started_at: Option<String>,
	pub ended_at: Option<String>,
	pub error_kind: Option<String>,
	pub error_message: Option<String>,
	pub error_retryable: Option<bool>,
}
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
