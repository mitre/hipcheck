//! `SeaORM` Entity, generated for the `package_version` table.

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(schema_name = "public", table_name = "package_version")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub package_id: i32,
    pub source_id: i32,
    pub version: String,
    pub package_url: String,
    pub source_repository: Option<String>,
    pub source_repository_tag: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
