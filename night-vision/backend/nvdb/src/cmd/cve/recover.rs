use anyhow::{Context as _, Result, bail};
use nv_common::{
	config::Config,
	cve::storage::{
		FailRunningCveListSyncRunsSummary, fail_running_cve_list_sync_runs,
		try_acquire_cve_list_sync_lock,
	},
	db, rt,
};
use sea_orm::TransactionTrait as _;

use crate::destructive::DestructiveOperationToken;

const STALE_RUNNING_SYNC_ERROR: &str =
	"sync run was marked failed by `nvdb cve recover` after no live sync lock was held";

pub fn command() -> clap::Command {
	clap::Command::new("recover")
		.about("Recover from interrupted CVE List sync metadata")
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

	runtime.block_on(recover(config, token))
}

async fn recover(config: &Config, _token: DestructiveOperationToken) -> Result<()> {
	let db = db::connection(config)
		.await
		.context("failed to connect to database")?;
	let transaction = db
		.begin()
		.await
		.context("failed to start CVE List recovery transaction")?;

	if !try_acquire_cve_list_sync_lock(&transaction).await? {
		transaction
			.rollback()
			.await
			.context("failed to roll back CVE List recovery transaction")?;
		bail!(
			"CVE List sync lock is held by an active database session. \
             Stop the active sync or wait for it to finish; transaction-level advisory locks \
             cannot be released by another session."
		);
	}

	let summary = fail_running_cve_list_sync_runs(&transaction, STALE_RUNNING_SYNC_ERROR)
		.await
		.context("failed to mark stale CVE List sync runs failed")?;
	transaction
		.commit()
		.await
		.context("failed to commit CVE List recovery transaction")?;

	print_summary(&summary);

	Ok(())
}

fn print_summary(summary: &FailRunningCveListSyncRunsSummary) {
	println!("sync_lock_available: true");
	println!(
		"stale_running_sync_runs_failed: {}",
		summary.sync_runs_failed
	);
}

#[cfg(test)]
mod tests {
	use super::command;
	use clap::error::ErrorKind;

	#[test]
	fn cve_recover_requires_destructive_flag() {
		let error = command()
			.try_get_matches_from(["recover"])
			.expect_err("missing destructive flag should fail");

		assert_eq!(error.kind(), ErrorKind::MissingRequiredArgument);
	}

	#[test]
	fn cve_recover_accepts_destructive_flag() {
		command()
			.try_get_matches_from(["recover", "--destructive"])
			.expect("destructive flag should parse");
	}
}
