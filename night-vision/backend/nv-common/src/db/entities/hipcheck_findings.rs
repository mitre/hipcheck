//! `SeaORM` entity for assessment-facing Hipcheck findings.
use sea_orm::entity::prelude::*;
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "hipcheck_findings")]
pub struct Model {
	#[sea_orm(primary_key)]
	pub id: i32,
	pub run_id: i32,
	pub check_id: Option<i32>,
	pub ordinal: i32,
	pub kind: String,
	pub effect: String,
	pub severity: Option<String>,
	pub summary: String,
	pub evidence: Json,
}
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
