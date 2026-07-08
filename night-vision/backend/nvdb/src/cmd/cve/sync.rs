use anyhow::{Context as _, Result};
use nv_common::{
    config::Config,
    cve::{
        git::{CommitSha, GitCliCveListGit},
        sync::{CveListSyncSummary, sync_cve_list_once},
    },
    db, rt,
};

use crate::destructive::DestructiveOperationToken;

pub fn command() -> clap::Command {
    clap::Command::new("sync")
        .about("Sync CVE List data once")
        .arg(
            clap::Arg::new("destructive")
                .short('w')
                .long("destructive")
                .required(true)
                .action(clap::ArgAction::SetTrue)
                .help("Acknowledge this command may modify database state"),
        )
}

pub fn run(config: &Config, matches: &clap::ArgMatches) -> Result<()> {
    let token = DestructiveOperationToken::new(matches);
    let runtime = rt::AsyncRuntime::new(config).context("failed to create async runtime")?;
    runtime.block_on(sync(config, token))
}

async fn sync(config: &Config, _token: DestructiveOperationToken) -> Result<()> {
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

#[cfg(test)]
mod tests {
    use super::command;
    use clap::error::ErrorKind;

    #[test]
    fn cve_sync_requires_destructive_flag() {
        let error = command()
            .try_get_matches_from(["sync"])
            .expect_err("missing destructive flag should fail");

        assert_eq!(error.kind(), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn cve_sync_accepts_destructive_flag() {
        command()
            .try_get_matches_from(["sync", "--destructive"])
            .expect("destructive flag should parse");
    }
}
