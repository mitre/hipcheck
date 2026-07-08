use anyhow::{Context as _, Result};
use nv_common::{
    config::Config,
    cve::{git::CommitSha, storage::last_successful_cve_list_sync_commit},
    db::{
        self,
        entities::{cve_list_sync_runs, cve_list_sync_runs::Model as CveListSyncRun},
    },
    rt,
};
use sea_orm::{EntityTrait as _, QueryOrder as _, QuerySelect as _};

pub fn command() -> clap::Command {
    clap::Command::new("status").about("Print the latest CVE List sync state")
}

pub fn run(config: &Config, _matches: &clap::ArgMatches) -> Result<()> {
    let runtime = rt::AsyncRuntime::new(config).context("failed to create async runtime")?;
    runtime.block_on(status(config))
}

async fn status(config: &Config) -> Result<()> {
    let db = db::connection(config)
        .await
        .context("failed to connect to database")?;
    let latest_successful_commit = last_successful_cve_list_sync_commit(&db)
        .await
        .context("failed to read latest successful CVE List sync commit")?;
    let latest_run = cve_list_sync_runs::Entity::find()
        .order_by_desc(cve_list_sync_runs::Column::Generation)
        .limit(1)
        .one(&db)
        .await
        .context("failed to read latest CVE List sync run")?;

    print_status(latest_successful_commit.as_ref(), latest_run.as_ref());

    Ok(())
}

fn print_status(latest_successful_commit: Option<&CommitSha>, latest_run: Option<&CveListSyncRun>) {
    println!(
        "latest_successful_commit: {}",
        latest_successful_commit.map_or("<none>", CommitSha::as_str)
    );

    if let Some(latest_run) = latest_run {
        println!("latest_run_generation: {}", latest_run.generation);
        println!("latest_run_status: {}", latest_run.status);
        println!("latest_run_checked_at: {}", latest_run.checked_at);
        println!(
            "latest_run_completed_at: {}",
            latest_run
                .completed_at
                .as_ref()
                .map_or("<none>".to_owned(), ToString::to_string)
        );
        println!("latest_run_records_seen: {}", latest_run.records_seen);
        println!(
            "latest_run_records_inserted: {}",
            latest_run.records_inserted
        );
        println!("latest_run_records_updated: {}", latest_run.records_updated);
        println!(
            "latest_run_error: {}",
            latest_run.error.as_deref().unwrap_or("<none>")
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
