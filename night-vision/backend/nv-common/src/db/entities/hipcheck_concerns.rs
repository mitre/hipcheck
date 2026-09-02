//! `SeaORM` entity for normalized Hipcheck concerns.
use sea_orm::entity::prelude::*;
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "hipcheck_concerns")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub check_id: i32,
    pub ordinal: i32,
    pub kind: String,
    pub message: String,
    pub details: Option<Json>,
}
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
