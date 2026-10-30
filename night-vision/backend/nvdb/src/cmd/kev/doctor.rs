use anyhow::{Context as _, Result, bail};
use chrono::Utc;
use nv_common::{
	config::Config,
	db::{self, entities::cisa_kev_sync_runs},
	kev::DEFAULT_KEV_REFRESH_INTERVAL_MILLISECONDS,
	rt,
};
use sea_orm::{
	ColumnTrait as _, DatabaseConnection, EntityTrait as _, QueryFilter as _, QueryOrder as _,
	QuerySelect as _,
};

use crate::time_display::TimeDisplay;

pub fn command() -> clap::Command {
	clap::Command::new("doctor").about("Check KEV cache freshness and sync health")
}

pub fn run(config: &Config, _matches: &clap::ArgMatches, time_display: TimeDisplay) -> Result<()> {
	let refresh_interval_ms = config
		.kev_refresh_interval
		.unwrap_or(DEFAULT_KEV_REFRESH_INTERVAL_MILLISECONDS);
	let runtime = rt::AsyncRuntime::new(config).context("failed to create async runtime")?;

	runtime.block_on(doctor(config, refresh_interval_ms, time_display))
}

async fn doctor(
	config: &Config,
	refresh_interval_ms: u64,
	time_display: TimeDisplay,
) -> Result<()> {
	let db = match db::connection(config).await {
		Ok(db) => {
			println!("database_reachable: true");
			db
		}
		Err(_) => {
			println!("database_reachable: false");
			println!("doctor_ok: false");
			bail!("KEV doctor could not connect to the database");
		}
	};
	let state = read_state(&db, refresh_interval_ms).await?;

	print_state(&state, time_display);
	if !state.ok {
		bail!("KEV doctor found failing checks");
	}

	Ok(())
}

async fn read_state(db: &DatabaseConnection, refresh_interval_ms: u64) -> Result<DoctorState> {
	let latest_run = cisa_kev_sync_runs::Entity::find()
		.order_by_desc(cisa_kev_sync_runs::Column::Generation)
		.limit(1)
		.one(db)
		.await
		.context("failed to read latest KEV sync run")?;
	let latest_successful_run = cisa_kev_sync_runs::Entity::find()
		.filter(cisa_kev_sync_runs::Column::Status.is_in(["success", "not_modified"]))
		.order_by_desc(cisa_kev_sync_runs::Column::Generation)
		.limit(1)
		.one(db)
		.await
		.context("failed to read latest successful KEV sync run")?;
	let latest_failure = cisa_kev_sync_runs::Entity::find()
		.filter(cisa_kev_sync_runs::Column::Status.eq("failed"))
		.order_by_desc(cisa_kev_sync_runs::Column::Generation)
		.limit(1)
		.one(db)
		.await
		.context("failed to read latest failed KEV sync run")?;
	let running_runs = cisa_kev_sync_runs::Entity::find()
		.filter(cisa_kev_sync_runs::Column::Status.eq("running"))
		.order_by_desc(cisa_kev_sync_runs::Column::Generation)
		.all(db)
		.await
		.context("failed to read running KEV sync runs")?;

	let latest_successful_age_seconds = latest_successful_run
		.as_ref()
		.map(|run| run_age_seconds(run.checked_at));
	let cache_validators_present = latest_successful_run.as_ref().is_some_and(|run| {
		run.etag.as_deref().is_some_and(|etag| !etag.is_empty())
			|| run
				.last_modified
				.as_deref()
				.is_some_and(|last_modified| !last_modified.is_empty())
	});
	let stale_running_run_count = running_runs
		.iter()
		.filter(|run| run_is_stale(run.checked_at, refresh_interval_ms))
		.count();
	let latest_run_failed = latest_run
		.as_ref()
		.is_some_and(|run| run.status == "failed");
	let cache_fresh = latest_successful_run
		.as_ref()
		.is_some_and(|run| !run_is_stale(run.checked_at, refresh_interval_ms));

	Ok(DoctorState {
		latest_run,
		latest_successful_run,
		latest_successful_age_seconds,
		cache_fresh,
		latest_failure,
		cache_validators_present,
		running_run_count: running_runs.len(),
		stale_running_run_count,
		ok: cache_fresh && !latest_run_failed && stale_running_run_count == 0,
	})
}

fn run_age_seconds(checked_at: chrono::DateTime<chrono::FixedOffset>) -> i64 {
	Utc::now()
		.signed_duration_since(checked_at)
		.num_seconds()
		.max(0)
}

fn run_is_stale(
	checked_at: chrono::DateTime<chrono::FixedOffset>,
	refresh_interval_ms: u64,
) -> bool {
	let refresh_interval_ms = i64::try_from(refresh_interval_ms)
		.expect("configured KEV refresh interval should fit in i64");

	Utc::now().signed_duration_since(checked_at)
		> chrono::Duration::milliseconds(refresh_interval_ms)
}

fn print_state(state: &DoctorState, time_display: TimeDisplay) {
	print_run("latest_run", state.latest_run.as_ref(), time_display);
	print_run(
		"latest_successful_run",
		state.latest_successful_run.as_ref(),
		time_display,
	);
	println!(
		"latest_successful_run_age_seconds: {}",
		state
			.latest_successful_age_seconds
			.map_or("<none>".to_owned(), |age| age.to_string())
	);
	println!("cache_fresh: {}", state.cache_fresh);
	print_failure(state.latest_failure.as_ref(), time_display);
	println!(
		"cache_validators_present: {}",
		state.cache_validators_present
	);
	println!("running_run_count: {}", state.running_run_count);
	println!("stale_running_run_count: {}", state.stale_running_run_count);
	println!("doctor_ok: {}", state.ok);
}

fn print_run(prefix: &str, run: Option<&cisa_kev_sync_runs::Model>, time_display: TimeDisplay) {
	if let Some(run) = run {
		println!("{prefix}_generation: {}", run.generation);
		println!("{prefix}_status: {}", run.status);
		println!(
			"{prefix}_checked_at: {}",
			time_display.format(&run.checked_at)
		);
	} else {
		println!("{prefix}_generation: <none>");
		println!("{prefix}_status: <none>");
		println!("{prefix}_checked_at: <none>");
	}
}

fn print_failure(run: Option<&cisa_kev_sync_runs::Model>, time_display: TimeDisplay) {
	if let Some(run) = run {
		println!("latest_failure_generation: {}", run.generation);
		println!(
			"latest_failure_checked_at: {}",
			time_display.format(&run.checked_at)
		);
		println!("latest_failure_error_present: {}", run.error.is_some());
	} else {
		println!("latest_failure_generation: <none>");
		println!("latest_failure_checked_at: <none>");
		println!("latest_failure_error_present: false");
	}
}

struct DoctorState {
	latest_run: Option<cisa_kev_sync_runs::Model>,
	latest_successful_run: Option<cisa_kev_sync_runs::Model>,
	latest_successful_age_seconds: Option<i64>,
	cache_fresh: bool,
	latest_failure: Option<cisa_kev_sync_runs::Model>,
	cache_validators_present: bool,
	running_run_count: usize,
	stale_running_run_count: usize,
	ok: bool,
}

#[cfg(test)]
mod tests {
	use super::command;

	#[test]
	fn kev_doctor_accepts_no_extra_arguments() {
		command()
			.try_get_matches_from(["doctor"])
			.expect("doctor subcommand should parse");
	}
}
