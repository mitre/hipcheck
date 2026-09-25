use anyhow::{Context as _, Result};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use nv_common::{
	config::Config,
	cve::{
		git::{CommitSha, GitCliCveListGit},
		progress::{CveListSyncProgress, CveListSyncProgressReporter},
		sync::{
			CveListSyncSummary, sync_cve_list_once_exclusive_with_progress_and_pipeline_config,
			sync_cve_list_once_exclusive_with_progress_pipeline_config_and_timeout,
		},
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
		.arg(
			clap::Arg::new("no-timeout")
				.long("no-timeout")
				.action(clap::ArgAction::SetTrue)
				.help("Disable the CVE List sync timeout for this run"),
		)
}

pub fn run(config: &Config, matches: &clap::ArgMatches) -> Result<()> {
	let token = DestructiveOperationToken::new(matches);
	let no_timeout = matches.get_flag("no-timeout");
	let runtime = rt::AsyncRuntime::new(config).context("failed to create async runtime")?;
	runtime.block_on(sync(config, token, no_timeout))
}

async fn sync(config: &Config, _token: DestructiveOperationToken, no_timeout: bool) -> Result<()> {
	let worker_config = config.cve_list_worker_config();
	let db = db::connection(config)
		.await
		.context("failed to connect to database")?;
	let git = GitCliCveListGit::new_with_record_max_bytes(
		worker_config.checkout_path().to_owned(),
		worker_config.repository_url().clone(),
		worker_config.repository_ref().clone(),
		worker_config.record_max_bytes(),
	);
	let progress = CveSyncProgressBar::new();
	let sync_result = if no_timeout {
		sync_cve_list_once_exclusive_with_progress_and_pipeline_config(
			&db,
			&git,
			worker_config.repository_url(),
			worker_config.repository_ref(),
			&progress,
			worker_config.sync_pipeline_config(),
		)
		.await
	} else {
		sync_cve_list_once_exclusive_with_progress_pipeline_config_and_timeout(
			&db,
			&git,
			worker_config.repository_url(),
			worker_config.repository_ref(),
			&progress,
			worker_config.sync_pipeline_config(),
			worker_config.sync_timeout_config(),
		)
		.await
	};

	progress.finish();

	let summary = sync_result.context("failed to sync CVE List data")?;

	print_summary(&summary);

	Ok(())
}

struct CveSyncProgressBar {
	multi: MultiProgress,
	read_bar: ProgressBar,
	parse_bar: ProgressBar,
	write_bar: ProgressBar,
}

impl CveSyncProgressBar {
	fn new() -> Self {
		let multi = MultiProgress::new();
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
			read_bar,
			parse_bar,
			write_bar,
		}
	}

	fn finish(&self) {
		let _ = self.multi.clear();
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
			CveListSyncProgress::RecordWriteStarted { records } => {
				self.reset_write_bar(records);
			}
			CveListSyncProgress::RecordWriteBatchCompleted { written, total } => {
				self.set_write_position(written, total);
			}
			CveListSyncProgress::Started { .. }
			| CveListSyncProgress::GitCheckoutStarted
			| CveListSyncProgress::GitFetchStarted
			| CveListSyncProgress::GitRefResolveStarted
			| CveListSyncProgress::PreviousSyncLookupStarted
			| CveListSyncProgress::CveFileListStarted
			| CveListSyncProgress::ExistingRecordLookupStarted { .. }
			| CveListSyncProgress::ExistingRecordLookupCompleted { .. }
			| CveListSyncProgress::NotModified
			| CveListSyncProgress::FinishStarted
			| CveListSyncProgress::Finished => {}
		}
	}
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

	#[test]
	fn cve_sync_accepts_no_timeout_flag() {
		let matches = command()
			.try_get_matches_from(["sync", "--destructive", "--no-timeout"])
			.expect("no-timeout flag should parse");

		assert!(matches.get_flag("no-timeout"));
	}
}
