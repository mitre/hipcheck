//! `SeaORM` Entity, generated for source-scoped elaboration warnings.

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(schema_name = "public", table_name = "package_source_warnings")]
pub struct Model {
	#[sea_orm(primary_key)]
	pub id: i32,
	pub source_id: i32,
	pub declared_by_purl: Option<String>,
	pub dependency_name: String,
	pub specification_kind: String,
	pub message: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
