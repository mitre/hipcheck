use anyhow::{Context as _, Result};
use indicatif::{ProgressBar, ProgressStyle};
use nv_common::{
    config::Config,
    cve::{
        git::{CommitSha, GitCliCveListGit},
        progress::{CveListSyncProgress, CveListSyncProgressReporter},
        sync::{CveListSyncSummary, sync_cve_list_once_with_progress},
    },
    db, rt,
};
use std::time::Duration;

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
    let progress = CveSyncProgressBar::new();
    let sync_result = sync_cve_list_once_with_progress(
        &db,
        &git,
        worker_config.repository_url(),
        worker_config.repository_ref(),
        worker_config.write_batch_size(),
        &progress,
    )
    .await;

    progress.finish();

    let summary = sync_result.context("failed to sync CVE List data")?;

    print_summary(&summary);

    Ok(())
}

struct CveSyncProgressBar {
    bar: ProgressBar,
}

impl CveSyncProgressBar {
    fn new() -> Self {
        let bar = ProgressBar::new_spinner();
        bar.set_style(spinner_style());
        bar.enable_steady_tick(Duration::from_millis(100));

        Self { bar }
    }

    fn finish(&self) {
        self.bar.finish_and_clear();
    }

    fn set_spinner_message(&self, message: &'static str) {
        self.bar.set_style(spinner_style());
        self.bar.enable_steady_tick(Duration::from_millis(100));
        self.bar.set_message(message);
    }

    fn set_bar(&self, len: usize, message: &'static str) {
        self.bar.set_style(bar_style());
        self.bar.set_length(len as u64);
        self.bar.set_position(0);
        self.bar.set_message(message);
    }
}

impl CveListSyncProgressReporter for CveSyncProgressBar {
    fn report(&self, progress: CveListSyncProgress) {
        match progress {
            CveListSyncProgress::Started { generation } => {
                self.bar
                    .set_message(format!("started CVE List sync run {generation}"));
            }
            CveListSyncProgress::GitCheckoutStarted => {
                self.set_spinner_message("checking CVE List checkout");
            }
            CveListSyncProgress::GitFetchStarted => {
                self.set_spinner_message("fetching CVE List repository");
            }
            CveListSyncProgress::GitRefResolveStarted => {
                self.set_spinner_message("resolving CVE List ref");
            }
            CveListSyncProgress::PreviousSyncLookupStarted => {
                self.set_spinner_message("checking previous sync");
            }
            CveListSyncProgress::CveFileListStarted => {
                self.set_spinner_message("listing CVE records");
            }
            CveListSyncProgress::CveFileListCompleted { records } => {
                self.set_bar(records, "parsing CVE records");
            }
            CveListSyncProgress::CveFileParsed { parsed, total } => {
                self.bar.set_length(total as u64);
                self.bar.set_position(parsed as u64);
            }
            CveListSyncProgress::ExistingRecordLookupStarted { records } => {
                self.set_spinner_message(if records == 1 {
                    "checking existing CVE record"
                } else {
                    "checking existing CVE records"
                });
            }
            CveListSyncProgress::ExistingRecordLookupCompleted { .. } => {
                self.set_spinner_message("preparing database writes");
            }
            CveListSyncProgress::RecordWriteStarted { records } => {
                self.set_bar(records, "writing CVE records");
            }
            CveListSyncProgress::RecordWriteBatchCompleted { written, total } => {
                self.bar.set_length(total as u64);
                self.bar.set_position(written as u64);
            }
            CveListSyncProgress::NotModified => {
                self.set_spinner_message("CVE List data is already current");
            }
            CveListSyncProgress::FinishStarted => {
                self.set_spinner_message("finalizing sync run");
            }
            CveListSyncProgress::Finished => {
                self.set_spinner_message("finished CVE List sync");
            }
        }
    }
}

fn spinner_style() -> ProgressStyle {
    ProgressStyle::with_template("{spinner:.green} {msg}")
        .expect("spinner progress template should be valid")
}

fn bar_style() -> ProgressStyle {
    ProgressStyle::with_template("{wide_bar:.cyan/blue} {pos}/{len} {msg}")
        .expect("bar progress template should be valid")
        .progress_chars("=> ")
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
