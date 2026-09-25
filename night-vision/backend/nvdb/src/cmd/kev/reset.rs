use anyhow::{Context as _, Result, anyhow};
use nv_common::{
	config::Config,
	db,
	kev::{ResetKevStorageError, ResetKevStorageSummary, reset_kev_storage},
	rt,
};
use sea_orm::TransactionTrait as _;

use crate::destructive::DestructiveOperationToken;

pub fn command() -> clap::Command {
	clap::Command::new("reset")
		.about("Clear cached KEV entries and sync state")
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
				.help("Reset even when a KEV sync run is marked running"),
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
		.context("failed to start KEV reset transaction")?;
	let summary = match reset_kev_storage(&transaction, force).await {
		Ok(summary) => summary,
		Err(error) => {
			transaction
				.rollback()
				.await
				.context("failed to roll back KEV reset transaction")?;
			return Err(reset_storage_error(error));
		}
	};

	transaction
		.commit()
		.await
		.context("failed to commit KEV reset transaction")?;
	print_summary(&summary);

	Ok(())
}

fn print_summary(summary: &ResetKevStorageSummary) {
	println!("kev_entries_deleted: {}", summary.entries_deleted);
	println!("sync_runs_deleted: {}", summary.sync_runs_deleted);
}

fn reset_storage_error(error: ResetKevStorageError) -> anyhow::Error {
	match error {
		ResetKevStorageError::RunningSyncRuns(count) => anyhow!(
			"refusing to reset KEV storage while {count} sync run(s) are running. \
             Stop nv-server or wait for the sync to finish, then retry. To override this guard, \
             run `nvdb kev reset --destructive --force`; this is unsafe because forcing a reset \
             while a sync is running can leave KEV cache state inconsistent."
		),
		ResetKevStorageError::SyncAlreadyRunning => anyhow!(
			"refusing to reset KEV storage while a live sync holds the advisory lock. \
             Stop nv-server or wait for the sync to finish, then retry."
		),
		error => anyhow!(error).context("failed to reset KEV storage"),
	}
}

#[cfg(test)]
mod tests {
	use super::{command, reset_storage_error};
	use clap::error::ErrorKind;
	use nv_common::kev::ResetKevStorageError;

	#[test]
	fn kev_reset_requires_destructive_flag() {
		let error = command()
			.try_get_matches_from(["reset"])
			.expect_err("missing destructive flag should fail");

		assert_eq!(error.kind(), ErrorKind::MissingRequiredArgument);
	}

	#[test]
	fn kev_reset_accepts_destructive_flag() {
		command()
			.try_get_matches_from(["reset", "--destructive"])
			.expect("destructive flag should parse");
	}

	#[test]
	fn kev_reset_accepts_force_flag() {
		let matches = command()
			.try_get_matches_from(["reset", "--destructive", "--force"])
			.expect("force flag should parse");

		assert!(matches.get_flag("force"));
	}

	#[test]
	fn running_sync_error_points_to_force_and_warns_it_is_unsafe() {
		let error = reset_storage_error(ResetKevStorageError::RunningSyncRuns(2)).to_string();

		assert!(error.contains("2 sync run(s) are running"));
		assert!(error.contains("nvdb kev reset --destructive --force"));
		assert!(error.contains("unsafe"));
		assert!(error.contains("inconsistent"));
	}

	#[test]
	fn live_sync_lock_error_does_not_point_to_force() {
		let error = reset_storage_error(ResetKevStorageError::SyncAlreadyRunning).to_string();

		assert!(error.contains("live sync holds the advisory lock"));
		assert!(error.contains("wait for the sync to finish"));
		assert!(!error.contains("--force"));
	}
}
