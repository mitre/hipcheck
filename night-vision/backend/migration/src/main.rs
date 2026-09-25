use sea_orm_migration::cli;
use tokio::runtime::Runtime;

fn main() {
    let rt = Runtime::new().unwrap();
    rt.block_on(cli::run_cli(migration::Migrator));
}
