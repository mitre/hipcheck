use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(schema_name = "public", table_name = "package_source_deletion_audits")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub source_id: String,
    pub reason: String,
    pub deleted_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
