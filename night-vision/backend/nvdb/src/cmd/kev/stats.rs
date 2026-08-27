use anyhow::{Context as _, Result, anyhow};
use chrono::{DateTime, FixedOffset};
use nv_common::{config::Config, db, rt};
use sea_orm::{ConnectionTrait, DatabaseBackend, QueryResult, Statement};

use crate::time_display::TimeDisplay;

pub fn command() -> clap::Command {
    clap::Command::new("stats").about("Summarize cached KEV entries and sync runs")
}

pub fn run(config: &Config, _matches: &clap::ArgMatches, time_display: TimeDisplay) -> Result<()> {
    let runtime = rt::AsyncRuntime::new(config).context("failed to create async runtime")?;

    runtime.block_on(stats(config, time_display))
}

async fn stats(config: &Config, time_display: TimeDisplay) -> Result<()> {
    let db = db::connection(config)
        .await
        .context("failed to connect to database")?;
    let summary = read_summary(&db).await?;

    print_stats(&summary, time_display);

    Ok(())
}

async fn read_summary<C>(db: &C) -> Result<Summary>
where
    C: ConnectionTrait,
{
    let statement = Statement::from_string(
        DatabaseBackend::Postgres,
        "SELECT \
             (SELECT COUNT(*) FROM public.cisa_kev_entries WHERE removed_at IS NULL) AS entries_total, \
             (SELECT COUNT(*) FROM public.cisa_kev_entries WHERE removed_at IS NOT NULL) AS entries_removed, \
             (SELECT MIN(first_seen_at) FROM public.cisa_kev_entries) \
                 AS first_seen_at_oldest, \
             (SELECT MAX(first_seen_at) FROM public.cisa_kev_entries) \
                 AS first_seen_at_newest, \
             (SELECT MIN(last_seen_at) FROM public.cisa_kev_entries) \
                 AS last_seen_at_oldest, \
             (SELECT MAX(last_seen_at) FROM public.cisa_kev_entries) \
                 AS last_seen_at_newest, \
             (SELECT MIN(updated_at) FROM public.cisa_kev_entries) \
                 AS updated_at_oldest, \
             (SELECT MAX(updated_at) FROM public.cisa_kev_entries) \
                 AS updated_at_newest, \
             COUNT(*) AS sync_runs_total, \
             COUNT(*) FILTER (WHERE status = 'success') AS sync_runs_success, \
             COUNT(*) FILTER (WHERE status = 'not_modified') AS sync_runs_not_modified, \
             COUNT(*) FILTER (WHERE status = 'failed') AS sync_runs_failed, \
             COUNT(*) FILTER (WHERE status = 'running') AS sync_runs_running, \
             COALESCE(SUM(records_seen), 0) AS records_seen_total, \
             COALESCE(SUM(records_inserted), 0) AS records_inserted_total, \
             COALESCE(SUM(records_updated), 0) AS records_updated_total, \
             MIN(checked_at) AS sync_checked_at_oldest, \
             MAX(checked_at) AS sync_checked_at_newest, \
             MAX(completed_at) AS sync_completed_at_newest \
         FROM public.cisa_kev_sync_runs"
            .to_owned(),
    );
    let result = db
        .query_one_raw(statement)
        .await
        .context("failed to read KEV statistics")?
        .ok_or_else(|| anyhow!("KEV statistics query returned no row"))?;

    Summary::try_from_query_result(&result)
}

struct Summary {
    entries_total: i64,
    entries_removed: i64,
    first_seen_at_oldest: Option<DateTime<FixedOffset>>,
    first_seen_at_newest: Option<DateTime<FixedOffset>>,
    last_seen_at_oldest: Option<DateTime<FixedOffset>>,
    last_seen_at_newest: Option<DateTime<FixedOffset>>,
    updated_at_oldest: Option<DateTime<FixedOffset>>,
    updated_at_newest: Option<DateTime<FixedOffset>>,
    sync_runs_total: i64,
    sync_runs_success: i64,
    sync_runs_not_modified: i64,
    sync_runs_failed: i64,
    sync_runs_running: i64,
    records_seen_total: i64,
    records_inserted_total: i64,
    records_updated_total: i64,
    sync_checked_at_oldest: Option<DateTime<FixedOffset>>,
    sync_checked_at_newest: Option<DateTime<FixedOffset>>,
    sync_completed_at_newest: Option<DateTime<FixedOffset>>,
}

impl Summary {
    fn try_from_query_result(result: &QueryResult) -> Result<Self> {
        Ok(Self {
            entries_total: result
                .try_get_by_index(0)
                .context("failed to read entry count")?,
            entries_removed: result
                .try_get_by_index(1)
                .context("failed to read removed entry count")?,
            first_seen_at_oldest: result
                .try_get_by_index(2)
                .context("failed to read oldest entry first_seen_at")?,
            first_seen_at_newest: result
                .try_get_by_index(3)
                .context("failed to read newest entry first_seen_at")?,
            last_seen_at_oldest: result
                .try_get_by_index(4)
                .context("failed to read oldest entry last_seen_at")?,
            last_seen_at_newest: result
                .try_get_by_index(5)
                .context("failed to read newest entry last_seen_at")?,
            updated_at_oldest: result
                .try_get_by_index(6)
                .context("failed to read oldest entry updated_at")?,
            updated_at_newest: result
                .try_get_by_index(7)
                .context("failed to read newest entry updated_at")?,
            sync_runs_total: result
                .try_get_by_index(8)
                .context("failed to read sync-run count")?,
            sync_runs_success: result
                .try_get_by_index(9)
                .context("failed to read successful sync-run count")?,
            sync_runs_not_modified: result
                .try_get_by_index(10)
                .context("failed to read not-modified sync-run count")?,
            sync_runs_failed: result
                .try_get_by_index(11)
                .context("failed to read failed sync-run count")?,
            sync_runs_running: result
                .try_get_by_index(12)
                .context("failed to read running sync-run count")?,
            records_seen_total: result
                .try_get_by_index(13)
                .context("failed to read records-seen total")?,
            records_inserted_total: result
                .try_get_by_index(14)
                .context("failed to read records-inserted total")?,
            records_updated_total: result
                .try_get_by_index(15)
                .context("failed to read records-updated total")?,
            sync_checked_at_oldest: result
                .try_get_by_index(16)
                .context("failed to read oldest sync checked_at")?,
            sync_checked_at_newest: result
                .try_get_by_index(17)
                .context("failed to read newest sync checked_at")?,
            sync_completed_at_newest: result
                .try_get_by_index(18)
                .context("failed to read newest sync completed_at")?,
        })
    }
}

fn print_stats(summary: &Summary, time_display: TimeDisplay) {
    println!("entries_active: {}", summary.entries_total);
    println!("entries_removed: {}", summary.entries_removed);
    print_timestamp(
        "entry_first_seen_at_oldest",
        summary.first_seen_at_oldest.as_ref(),
        time_display,
    );
    print_timestamp(
        "entry_first_seen_at_newest",
        summary.first_seen_at_newest.as_ref(),
        time_display,
    );
    print_timestamp(
        "entry_last_seen_at_oldest",
        summary.last_seen_at_oldest.as_ref(),
        time_display,
    );
    print_timestamp(
        "entry_last_seen_at_newest",
        summary.last_seen_at_newest.as_ref(),
        time_display,
    );
    print_timestamp(
        "entry_updated_at_oldest",
        summary.updated_at_oldest.as_ref(),
        time_display,
    );
    print_timestamp(
        "entry_updated_at_newest",
        summary.updated_at_newest.as_ref(),
        time_display,
    );
    println!("sync_runs_total: {}", summary.sync_runs_total);
    println!("sync_runs_success: {}", summary.sync_runs_success);
    println!("sync_runs_not_modified: {}", summary.sync_runs_not_modified);
    println!("sync_runs_failed: {}", summary.sync_runs_failed);
    println!("sync_runs_running: {}", summary.sync_runs_running);
    println!("records_seen_total: {}", summary.records_seen_total);
    println!("records_inserted_total: {}", summary.records_inserted_total);
    println!("records_updated_total: {}", summary.records_updated_total);
    print_timestamp(
        "sync_checked_at_oldest",
        summary.sync_checked_at_oldest.as_ref(),
        time_display,
    );
    print_timestamp(
        "sync_checked_at_newest",
        summary.sync_checked_at_newest.as_ref(),
        time_display,
    );
    print_timestamp(
        "sync_completed_at_newest",
        summary.sync_completed_at_newest.as_ref(),
        time_display,
    );
}

fn print_timestamp(
    label: &str,
    timestamp: Option<&DateTime<FixedOffset>>,
    time_display: TimeDisplay,
) {
    println!("{label}: {}", time_display.format_optional(timestamp));
}

#[cfg(test)]
mod tests {
    use super::command;
    use crate::time_display::TimeDisplay;

    #[test]
    fn kev_stats_accepts_no_arguments() {
        command()
            .try_get_matches_from(["stats"])
            .expect("stats should parse");
    }

    #[test]
    fn optional_timestamp_uses_none_marker() {
        assert_eq!(TimeDisplay::Stored.format_optional(None), "<none>");
    }
}
