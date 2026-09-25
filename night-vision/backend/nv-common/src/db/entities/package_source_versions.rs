//! `SeaORM` Entity, generated for source-scoped package-version reachability.

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(schema_name = "public", table_name = "package_source_versions")]
pub struct Model {
	#[sea_orm(primary_key)]
	pub id: i32,
	pub source_id: i32,
	pub package_version_id: i32,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
