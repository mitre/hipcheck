pub use sea_orm_migration::{MigrationTrait, MigratorTrait, async_trait};

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260623_161612_initial_schema::Migration),
            Box::new(m20260630_173030_create_package_table::Migration),
            Box::new(m20260706_000000_create_cisa_kev_tables::Migration),
            Box::new(m20260707_000000_create_cve_list_tables::Migration),
            Box::new(m20260708_150958_create_package_version_table::Migration),
            Box::new(m20260710_000000_add_cve_list_record_deleted::Migration),
            Box::new(m20260710_010000_create_cve_list_record_staging::Migration),
            Box::new(m20260711_000000_add_cisa_kev_entry_removed_at::Migration),
            Box::new(m20260903_000000_add_package_source_elaboration::Migration),
            Box::new(m20260903_010000_add_package_source_created_at::Migration),
        ]
    }
}
mod m20260623_161612_initial_schema;
mod m20260630_173030_create_package_table;
mod m20260706_000000_create_cisa_kev_tables;
mod m20260707_000000_create_cve_list_tables;
mod m20260708_150958_create_package_version_table;
mod m20260710_000000_add_cve_list_record_deleted;
mod m20260710_010000_create_cve_list_record_staging;
mod m20260711_000000_add_cisa_kev_entry_removed_at;
mod m20260903_000000_add_package_source_elaboration;
mod m20260903_010000_add_package_source_created_at;
