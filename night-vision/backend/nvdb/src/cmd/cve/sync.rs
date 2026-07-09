use anyhow::{Context as _, Result};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use nv_common::{
    config::Config,
    cve::{
        git::{CommitSha, GitCliCveListGit},
        progress::{CveListSyncProgress, CveListSyncProgressReporter},
        sync::{CveListSyncSummary, sync_cve_list_once_with_progress_and_pipeline_config},
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
    let sync_result = sync_cve_list_once_with_progress_and_pipeline_config(
        &db,
        &git,
        worker_config.repository_url(),
        worker_config.repository_ref(),
        &progress,
        worker_config.sync_pipeline_config(),
    )
    .await;

    progress.finish();

    let summary = sync_result.context("failed to sync CVE List data")?;

    print_summary(&summary);

    Ok(())
}

struct CveSyncProgressBar {
    multi: MultiProgress,
    status_bar: ProgressBar,
    read_bar: ProgressBar,
    parse_bar: ProgressBar,
    write_bar: ProgressBar,
}

impl CveSyncProgressBar {
    fn new() -> Self {
        let multi = MultiProgress::new();
        let status_bar = multi.add(ProgressBar::new_spinner());
        status_bar.set_style(spinner_style());
        status_bar.enable_steady_tick(Duration::from_millis(100));

        let read_bar = multi.add(ProgressBar::new(0));
        read_bar.set_style(bar_style());
        read_bar.set_message("reading CVE records");

        let parse_bar = multi.add(ProgressBar::new(0));
        parse_bar.set_style(bar_style());
        parse_bar.set_message("parsing CVE records");

        let write_bar = multi.add(ProgressBar::new(0));
        write_bar.set_style(bar_style());
        write_bar.set_message("writing CVE records");

        Self {
            multi,
            status_bar,
            read_bar,
            parse_bar,
            write_bar,
        }
    }

    fn finish(&self) {
        let _ = self.multi.clear();
    }

    fn set_spinner_message(&self, message: &'static str) {
        self.status_bar.set_style(spinner_style());
        self.status_bar
            .enable_steady_tick(Duration::from_millis(100));
        self.status_bar.set_message(message);
    }

    fn reset_parse_bar(&self, len: usize) {
        self.parse_bar.set_length(len as u64);
        self.parse_bar.set_position(0);
    }

    fn reset_read_bar(&self, len: usize) {
        self.read_bar.set_length(len as u64);
        self.read_bar.set_position(0);
    }

    fn set_read_position(&self, read: usize, total: usize) {
        self.read_bar.set_length(total as u64);
        self.read_bar.set_position(read as u64);
    }

    fn set_parse_position(&self, parsed: usize, total: usize) {
        self.parse_bar.set_length(total as u64);
        self.parse_bar.set_position(parsed as u64);
    }

    fn reset_write_bar(&self, len: usize) {
        self.write_bar.set_length(len as u64);
        self.write_bar.set_position(0);
    }

    fn set_write_position(&self, written: usize, total: usize) {
        self.write_bar.set_length(total as u64);
        self.write_bar.set_position(written as u64);
    }
}

impl CveListSyncProgressReporter for CveSyncProgressBar {
    fn report(&self, progress: CveListSyncProgress) {
        match progress {
            CveListSyncProgress::Started { generation } => {
                self.status_bar
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
                self.reset_parse_bar(records);
            }
            CveListSyncProgress::CveFileReadStarted { records } => {
                self.reset_read_bar(records);
            }
            CveListSyncProgress::CveFileReadCompleted { read, total } => {
                self.set_read_position(read, total);
            }
            CveListSyncProgress::CveFileParsed { parsed, total } => {
                self.set_parse_position(parsed, total);
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
                self.reset_write_bar(records);
            }
            CveListSyncProgress::RecordWriteBatchCompleted { written, total } => {
                self.set_write_position(written, total);
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
    ProgressStyle::with_template("{wide_bar:.cyan/blue} {human_pos}/{human_len} {msg}")
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
