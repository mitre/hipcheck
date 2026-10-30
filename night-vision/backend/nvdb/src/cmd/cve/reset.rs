use anyhow::{Context as _, Result, anyhow};
use nv_common::{
	config::Config,
	cve::storage::{ResetCveListStorageError, ResetCveListStorageSummary, reset_cve_list_storage},
	db, rt,
};
use sea_orm::TransactionTrait as _;

use crate::destructive::DestructiveOperationToken;

pub fn command() -> clap::Command {
	clap::Command::new("reset")
		.about("Clear ingested CVE List records and sync state")
		.arg(
			clap::Arg::new("destructive")
				.short('w')
				.long("destructive")
				.required(true)
				.action(clap::ArgAction::SetTrue)
				.help("Acknowledge this command may modify database state"),
		)
		.arg(
			clap::Arg::new("force")
				.long("force")
				.action(clap::ArgAction::SetTrue)
				.help("Reset even when a CVE List sync run is marked running"),
		)
}

pub fn run(config: &Config, matches: &clap::ArgMatches) -> Result<()> {
	let token = DestructiveOperationToken::new(matches);
	let force = matches.get_flag("force");
	let runtime = rt::AsyncRuntime::new(config).context("failed to create async runtime")?;
	runtime.block_on(reset(config, token, force))
}

async fn reset(config: &Config, _token: DestructiveOperationToken, force: bool) -> Result<()> {
	let db = db::connection(config)
		.await
		.context("failed to connect to database")?;
	let transaction = db
		.begin()
		.await
		.context("failed to start CVE List reset transaction")?;
	let summary = match reset_cve_list_storage(&transaction, force).await {
		Ok(summary) => summary,
		Err(error) => {
			transaction
				.rollback()
				.await
				.context("failed to roll back CVE List reset transaction")?;
			return Err(reset_storage_error(error));
		}
	};

	transaction
		.commit()
		.await
		.context("failed to commit CVE List reset transaction")?;
	print_summary(&summary);

	Ok(())
}

fn print_summary(summary: &ResetCveListStorageSummary) {
	println!("cve_records_deleted: {}", summary.records_deleted);
	println!("sync_runs_deleted: {}", summary.sync_runs_deleted);
}

fn reset_storage_error(error: ResetCveListStorageError) -> anyhow::Error {
	match error {
		ResetCveListStorageError::RunningSyncRuns(count) => anyhow!(
			"refusing to reset CVE List storage while {count} sync run(s) are running. \
             Stop nv-server or wait for the sync to finish, then retry. To override this guard, \
             run `nvdb cve reset --destructive --force`; this is unsafe because forcing a reset \
             while a sync is running can leave CVE ingestion state inconsistent."
		),
		ResetCveListStorageError::SyncAlreadyRunning => anyhow!(
			"refusing to reset CVE List storage while a live sync holds the advisory lock. \
             Stop nv-server or wait for the sync to finish, then retry."
		),
		error => anyhow!(error).context("failed to reset CVE List storage"),
	}
}

#[cfg(test)]
mod tests {
	use super::{command, reset_storage_error};
	use clap::error::ErrorKind;
	use nv_common::cve::storage::ResetCveListStorageError;

	#[test]
	fn cve_reset_requires_destructive_flag() {
		let error = command()
			.try_get_matches_from(["reset"])
			.expect_err("missing destructive flag should fail");

		assert_eq!(error.kind(), ErrorKind::MissingRequiredArgument);
	}

	#[test]
	fn cve_reset_accepts_destructive_flag() {
		command()
			.try_get_matches_from(["reset", "--destructive"])
			.expect("destructive flag should parse");
	}

	#[test]
	fn cve_reset_accepts_force_flag() {
		let matches = command()
			.try_get_matches_from(["reset", "--destructive", "--force"])
			.expect("force flag should parse");

		assert!(matches.get_flag("force"));
	}

	#[test]
	fn running_sync_error_points_to_force_and_warns_it_is_unsafe() {
		let error = reset_storage_error(ResetCveListStorageError::RunningSyncRuns(2)).to_string();

		assert!(error.contains("2 sync run(s) are running"));
		assert!(error.contains("nvdb cve reset --destructive --force"));
		assert!(error.contains("unsafe"));
		assert!(error.contains("inconsistent"));
	}

	#[test]
	fn live_sync_lock_error_does_not_point_to_force() {
		let error = reset_storage_error(ResetCveListStorageError::SyncAlreadyRunning).to_string();

		assert!(error.contains("live sync holds the advisory lock"));
		assert!(error.contains("wait for the sync to finish"));
		assert!(!error.contains("--force"));
	}
}
