pub use sea_orm_migration::{MigrationTrait, MigratorTrait, async_trait};

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260623_161612_initial_schema::Migration),
            Box::new(m20260706_000000_create_cisa_kev_tables::Migration),
        ]
    }
}
mod m20260623_161612_initial_schema;
mod m20260706_000000_create_cisa_kev_tables;
