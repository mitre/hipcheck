pub use sea_orm_migration::{MigrationTrait, MigratorTrait, async_trait};

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260623_161612_initial_schema::Migration),
            Box::new(m20260706_000000_create_cisa_kev_tables::Migration),
            Box::new(m20260707_000000_create_cve_list_tables::Migration),
            Box::new(m20260630_173030_create_package_table::Migration),
            Box::new(m20260708_150958_create_package_version_table::Migration),
        ]
    }
}
mod m20260623_161612_initial_schema;
mod m20260630_173030_create_package_table;
mod m20260706_000000_create_cisa_kev_tables;
mod m20260707_000000_create_cve_list_tables;
mod m20260708_150958_create_package_version_table;
