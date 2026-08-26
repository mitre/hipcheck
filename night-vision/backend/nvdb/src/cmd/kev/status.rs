use anyhow::{Context as _, Result};
use nv_common::{
    config::Config,
    db::{
        self,
        entities::{cisa_kev_entries, cisa_kev_sync_runs, cisa_kev_sync_runs::Model as KevSyncRun},
    },
    rt,
};
use sea_orm::{
    ColumnTrait as _, EntityTrait as _, PaginatorTrait as _, QueryFilter as _, QueryOrder as _,
    QuerySelect as _,
};

pub fn command() -> clap::Command {
    clap::Command::new("status").about("Print the latest KEV catalog sync state")
}

pub fn run(config: &Config, _matches: &clap::ArgMatches) -> Result<()> {
    let runtime = rt::AsyncRuntime::new(config).context("failed to create async runtime")?;
    runtime.block_on(status(config))
}

async fn status(config: &Config) -> Result<()> {
    let db = db::connection(config)
        .await
        .context("failed to connect to database")?;
    let entry_count = cisa_kev_entries::Entity::find()
        .count(&db)
        .await
        .context("failed to count cached KEV entries")?;
    let latest_run = cisa_kev_sync_runs::Entity::find()
        .order_by_desc(cisa_kev_sync_runs::Column::Generation)
        .limit(1)
        .one(&db)
        .await
        .context("failed to read latest KEV sync run")?;
    let latest_successful_run = cisa_kev_sync_runs::Entity::find()
        .filter(cisa_kev_sync_runs::Column::Status.is_in(["success", "not_modified"]))
        .order_by_desc(cisa_kev_sync_runs::Column::Generation)
        .limit(1)
        .one(&db)
        .await
        .context("failed to read latest successful KEV sync run")?;

    print_status(
        entry_count,
        latest_run.as_ref(),
        latest_successful_run.as_ref(),
    );

    Ok(())
}

fn print_status(
    entry_count: u64,
    latest_run: Option<&KevSyncRun>,
    latest_successful_run: Option<&KevSyncRun>,
) {
    println!("entry_count: {entry_count}");
    print_latest_run(latest_run);
    print_latest_successful_catalog(latest_successful_run);
}

fn print_latest_run(run: Option<&KevSyncRun>) {
    if let Some(run) = run {
        println!("latest_run_generation: {}", run.generation);
        println!("latest_run_status: {}", run.status);
        println!("latest_run_checked_at: {}", run.checked_at);
        println!(
            "latest_run_completed_at: {}",
            run.completed_at
                .as_ref()
                .map_or("<none>".to_owned(), ToString::to_string)
        );
        println!("latest_run_records_seen: {}", run.records_seen);
        println!("latest_run_records_inserted: {}", run.records_inserted);
        println!("latest_run_records_updated: {}", run.records_updated);
        println!(
            "latest_run_error: {}",
            run.error.as_deref().unwrap_or("<none>")
        );
    } else {
        println!("latest_run_generation: <none>");
        println!("latest_run_status: <none>");
        println!("latest_run_checked_at: <none>");
        println!("latest_run_completed_at: <none>");
        println!("latest_run_records_seen: <none>");
        println!("latest_run_records_inserted: <none>");
        println!("latest_run_records_updated: <none>");
        println!("latest_run_error: <none>");
    }
}

fn print_latest_successful_catalog(run: Option<&KevSyncRun>) {
    if let Some(run) = run {
        println!("latest_successful_generation: {}", run.generation);
        println!("latest_successful_status: {}", run.status);
        println!("latest_successful_checked_at: {}", run.checked_at);
        println!(
            "latest_successful_completed_at: {}",
            run.completed_at
                .as_ref()
                .map_or("<none>".to_owned(), ToString::to_string)
        );
        println!(
            "latest_successful_catalog_version: {}",
            run.catalog_version.as_deref().unwrap_or("<none>")
        );
        println!(
            "latest_successful_catalog_date_released: {}",
            run.catalog_date_released
                .as_ref()
                .map_or("<none>".to_owned(), ToString::to_string)
        );
        println!(
            "latest_successful_catalog_count: {}",
            run.catalog_count
                .map_or("<none>".to_owned(), |count| count.to_string())
        );
        println!(
            "latest_successful_content_sha256: {}",
            run.content_sha256.as_deref().unwrap_or("<none>")
        );
        println!(
            "latest_successful_etag: {}",
            run.etag.as_deref().unwrap_or("<none>")
        );
        println!(
            "latest_successful_last_modified: {}",
            run.last_modified.as_deref().unwrap_or("<none>")
        );
    } else {
        println!("latest_successful_generation: <none>");
        println!("latest_successful_status: <none>");
        println!("latest_successful_checked_at: <none>");
        println!("latest_successful_completed_at: <none>");
        println!("latest_successful_catalog_version: <none>");
        println!("latest_successful_catalog_date_released: <none>");
        println!("latest_successful_catalog_count: <none>");
        println!("latest_successful_content_sha256: <none>");
        println!("latest_successful_etag: <none>");
        println!("latest_successful_last_modified: <none>");
    }
}

#[cfg(test)]
mod tests {
    use super::command;

    #[test]
    fn kev_status_accepts_no_extra_arguments() {
        command()
            .try_get_matches_from(["status"])
            .expect("status subcommand should parse");
    }
}
