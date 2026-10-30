use anyhow::{Context as _, Result};
use chrono::Utc;
use nv_common::{
	config::Config,
	db,
	npm::elaboration::lifecycle::{
		CleanupSummary, DEFAULT_CLEANUP_BATCH_SIZE, MAX_CLEANUP_BATCH_SIZE, cleanup_expired,
	},
	rt,
};

pub fn command() -> clap::Command {
	clap::Command::new("cleanup")
		.about("Delete expired package sources and deletion audits in a bounded batch")
		.arg(
			clap::Arg::new("batch-size")
				.long("batch-size")
				.value_name("COUNT")
				.default_value("100")
				.value_parser(clap::value_parser!(u64).range(1..=MAX_CLEANUP_BATCH_SIZE)),
		)
		.arg(
			clap::Arg::new("dry-run")
				.long("dry-run")
				.action(clap::ArgAction::SetTrue)
				.help("Report eligible package sources without deleting them"),
		)
		.arg(
			clap::Arg::new("json")
				.long("json")
				.action(clap::ArgAction::SetTrue)
				.help("Print the cleanup summary as JSON"),
		)
}

pub fn run(config: &Config, matches: &clap::ArgMatches) -> Result<()> {
	let batch_size = *matches
		.get_one::<u64>("batch-size")
		.unwrap_or(&DEFAULT_CLEANUP_BATCH_SIZE);
	let dry_run = matches.get_flag("dry-run");
	let runtime = rt::AsyncRuntime::new(config).context("failed to create async runtime")?;
	let summary = runtime.block_on(run_cleanup(config, batch_size, dry_run))?;
	if matches.get_flag("json") {
		println!("{}", json_output(summary, dry_run));
	} else {
		println!("scanned: {}", summary.scanned);
		println!("deleted: {}", summary.deleted);
		println!("recovered: {}", summary.recovered);
		println!("skipped: {}", summary.skipped);
		println!("deletion_audits_scanned: {}", summary.audits_scanned);
		println!("deletion_audits_purged: {}", summary.audits_purged);
		println!("dry_run: {dry_run}");
	}
	Ok(())
}

async fn run_cleanup(config: &Config, batch_size: u64, dry_run: bool) -> Result<CleanupSummary> {
	let db = db::connection(config)
		.await
		.context("failed to connect to database")?;
	cleanup_expired(&db, Utc::now(), batch_size, dry_run)
		.await
		.context("package-source cleanup failed")
}

fn json_output(summary: CleanupSummary, dry_run: bool) -> serde_json::Value {
	serde_json::json!({
		"scanned": summary.scanned,
		"deleted": summary.deleted,
		"recovered": summary.recovered,
		"skipped": summary.skipped,
		"deletionAuditsScanned": summary.audits_scanned,
		"deletionAuditsPurged": summary.audits_purged,
		"dryRun": dry_run,
	})
}

#[cfg(test)]
mod tests {
	use super::{command, json_output};
	use nv_common::npm::elaboration::lifecycle::CleanupSummary;

	#[test]
	fn cleanup_arguments_are_bounded() {
		command()
			.try_get_matches_from(["cleanup", "--batch-size", "1000", "--dry-run"])
			.expect("maximum batch size is accepted");
		command()
			.try_get_matches_from(["cleanup", "--batch-size", "1001"])
			.expect_err("batch sizes above the bound must be rejected");
	}

	#[test]
	fn cleanup_json_is_observable() {
		let value = json_output(
			CleanupSummary {
				scanned: 3,
				deleted: 2,
				recovered: 1,
				skipped: 1,
				audits_scanned: 4,
				audits_purged: 4,
			},
			false,
		);
		assert_eq!(value["deleted"], 2);
		assert_eq!(value["deletionAuditsScanned"], 4);
		assert_eq!(value["deletionAuditsPurged"], 4);
		assert_eq!(value["dryRun"], false);
	}
}
