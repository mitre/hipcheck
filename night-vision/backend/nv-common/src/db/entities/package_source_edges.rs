//! `SeaORM` Entity, generated for source-scoped package dependency edges.

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(schema_name = "public", table_name = "package_source_edges")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub source_id: i32,
    pub parent_package_version_id: Option<i32>,
    pub child_package_version_id: i32,
    pub root_dependency_kind: Option<String>,
    pub declared_dependency: Option<String>,
    pub declared_specification: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
