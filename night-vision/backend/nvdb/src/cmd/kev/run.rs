use anyhow::{Context as _, Result, bail};
use nv_common::{
	config::Config,
	db::{
		self,
		entities::{cisa_kev_sync_runs, cisa_kev_sync_runs::Model as KevSyncRun},
	},
	rt,
};
use sea_orm::EntityTrait as _;

use crate::time_display::TimeDisplay;

pub fn command() -> clap::Command {
	clap::Command::new("run")
		.about("Show one KEV catalog sync run")
		.arg(
			clap::Arg::new("generation")
				.value_name("GENERATION")
				.required(true)
				.value_parser(clap::value_parser!(i64).range(1..))
				.help("Sync-run generation to show"),
		)
}

pub fn run(config: &Config, matches: &clap::ArgMatches, time_display: TimeDisplay) -> Result<()> {
	let generation = *matches
		.get_one::<i64>("generation")
		.expect("required generation argument");
	let runtime = rt::AsyncRuntime::new(config).context("failed to create async runtime")?;

	runtime.block_on(show_run(config, generation, time_display))
}

async fn show_run(config: &Config, generation: i64, time_display: TimeDisplay) -> Result<()> {
	let db = db::connection(config)
		.await
		.context("failed to connect to database")?;
	let run = cisa_kev_sync_runs::Entity::find_by_id(generation)
		.one(&db)
		.await
		.context("failed to read KEV catalog sync run")?;
	let Some(run) = run else {
		bail!("KEV catalog sync run not found: {generation}");
	};

	print_run(&run, time_display);

	Ok(())
}

fn print_run(run: &KevSyncRun, time_display: TimeDisplay) {
	println!("generation: {}", run.generation);
	println!("status: {}", run.status);
	println!(
		"catalog_version: {}",
		run.catalog_version.as_deref().unwrap_or("<none>")
	);
	println!(
		"catalog_date_released: {}",
		run.catalog_date_released
			.as_ref()
			.map_or("<none>".to_owned(), ToString::to_string)
	);
	println!(
		"catalog_count: {}",
		run.catalog_count
			.map_or("<none>".to_owned(), |count| count.to_string())
	);
	println!("etag: {}", run.etag.as_deref().unwrap_or("<none>"));
	println!(
		"last_modified: {}",
		run.last_modified.as_deref().unwrap_or("<none>")
	);
	println!(
		"content_sha256: {}",
		run.content_sha256.as_deref().unwrap_or("<none>")
	);
	println!("records_seen: {}", run.records_seen);
	println!("records_inserted: {}", run.records_inserted);
	println!("records_updated: {}", run.records_updated);
	println!("checked_at: {}", time_display.format(&run.checked_at));
	println!(
		"completed_at: {}",
		time_display.format_optional(run.completed_at.as_ref())
	);
	println!("error: {}", run.error.as_deref().unwrap_or("<none>"));
}

#[cfg(test)]
mod tests {
	use super::command;
	use clap::error::ErrorKind;

	#[test]
	fn kev_run_requires_a_generation() {
		let error = command()
			.try_get_matches_from(["run"])
			.expect_err("missing generation should fail");

		assert_eq!(error.kind(), ErrorKind::MissingRequiredArgument);
	}

	#[test]
	fn kev_run_accepts_a_positive_generation() {
		command()
			.try_get_matches_from(["run", "1"])
			.expect("positive generation should parse");
	}

	#[test]
	fn kev_run_rejects_zero_generation() {
		command()
			.try_get_matches_from(["run", "0"])
			.expect_err("zero generation should fail");
	}
}
