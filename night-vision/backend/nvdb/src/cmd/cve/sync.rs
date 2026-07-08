use anyhow::{Context as _, Result};
use nv_common::{
    config::Config,
    cve::{
        git::{CommitSha, GitCliCveListGit},
        sync::{CveListSyncSummary, sync_cve_list_once},
    },
    db, rt,
};

pub fn command() -> clap::Command {
    clap::Command::new("sync").about("Sync CVE List data once")
}

pub fn run(config: &Config, _matches: &clap::ArgMatches) -> Result<()> {
    let runtime = rt::AsyncRuntime::new(config).context("failed to create async runtime")?;
    runtime.block_on(sync(config))
}

async fn sync(config: &Config) -> Result<()> {
    let worker_config = config.cve_list_worker_config();
    let db = db::connection(config)
        .await
        .context("failed to connect to database")?;
    let git = GitCliCveListGit::new(
        worker_config.checkout_path().to_owned(),
        worker_config.repository_url().clone(),
        worker_config.repository_ref().clone(),
    );
    let summary = sync_cve_list_once(
        &db,
        &git,
        worker_config.repository_url(),
        worker_config.repository_ref(),
        worker_config.write_batch_size(),
    )
    .await
    .context("failed to sync CVE List data")?;

    print_summary(&summary);

    Ok(())
}

fn print_summary(summary: &CveListSyncSummary) {
    println!("generation: {}", summary.generation);
    println!("status: {}", summary.status);
    println!("commit_sha: {}", summary_commit_sha(summary));
    println!("records_seen: {}", summary.records_seen);
    println!("records_inserted: {}", summary.records_inserted);
    println!("records_updated: {}", summary.records_updated);
}

fn summary_commit_sha(summary: &CveListSyncSummary) -> &str {
    summary
        .commit_sha
        .as_ref()
        .map_or("<none>", CommitSha::as_str)
}
