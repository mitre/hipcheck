use anyhow::{Context as _, Result, anyhow};
use nv_common::{config::Config, db, rt};
use sea_orm::{ConnectionTrait, DatabaseBackend, QueryResult, Statement};

pub fn command() -> clap::Command {
    clap::Command::new("stats").about("Summarize cached KEV entries and sync runs")
}

pub fn run(config: &Config, _matches: &clap::ArgMatches) -> Result<()> {
    let runtime = rt::AsyncRuntime::new(config).context("failed to create async runtime")?;

    runtime.block_on(stats(config))
}

async fn stats(config: &Config) -> Result<()> {
    let db = db::connection(config)
        .await
        .context("failed to connect to database")?;
    let summary = read_summary(&db).await?;

    print_stats(&summary);

    Ok(())
}

async fn read_summary<C>(db: &C) -> Result<Summary>
where
    C: ConnectionTrait,
{
    let statement = Statement::from_string(
        DatabaseBackend::Postgres,
        "SELECT \
             (SELECT COUNT(*) FROM public.cisa_kev_entries) AS entries_total, \
             (SELECT MIN(first_seen_at)::text FROM public.cisa_kev_entries) \
                 AS first_seen_at_oldest, \
             (SELECT MAX(first_seen_at)::text FROM public.cisa_kev_entries) \
                 AS first_seen_at_newest, \
             (SELECT MIN(last_seen_at)::text FROM public.cisa_kev_entries) \
                 AS last_seen_at_oldest, \
             (SELECT MAX(last_seen_at)::text FROM public.cisa_kev_entries) \
                 AS last_seen_at_newest, \
             (SELECT MIN(updated_at)::text FROM public.cisa_kev_entries) \
                 AS updated_at_oldest, \
             (SELECT MAX(updated_at)::text FROM public.cisa_kev_entries) \
                 AS updated_at_newest, \
             COUNT(*) AS sync_runs_total, \
             COUNT(*) FILTER (WHERE status = 'success') AS sync_runs_success, \
             COUNT(*) FILTER (WHERE status = 'not_modified') AS sync_runs_not_modified, \
             COUNT(*) FILTER (WHERE status = 'failed') AS sync_runs_failed, \
             COUNT(*) FILTER (WHERE status = 'running') AS sync_runs_running, \
             COALESCE(SUM(records_seen), 0) AS records_seen_total, \
             COALESCE(SUM(records_inserted), 0) AS records_inserted_total, \
             COALESCE(SUM(records_updated), 0) AS records_updated_total, \
             MIN(checked_at)::text AS sync_checked_at_oldest, \
             MAX(checked_at)::text AS sync_checked_at_newest, \
             MAX(completed_at)::text AS sync_completed_at_newest \
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
    first_seen_at_oldest: Option<String>,
    first_seen_at_newest: Option<String>,
    last_seen_at_oldest: Option<String>,
    last_seen_at_newest: Option<String>,
    updated_at_oldest: Option<String>,
    updated_at_newest: Option<String>,
    sync_runs_total: i64,
    sync_runs_success: i64,
    sync_runs_not_modified: i64,
    sync_runs_failed: i64,
    sync_runs_running: i64,
    records_seen_total: i64,
    records_inserted_total: i64,
    records_updated_total: i64,
    sync_checked_at_oldest: Option<String>,
    sync_checked_at_newest: Option<String>,
    sync_completed_at_newest: Option<String>,
}

impl Summary {
    fn try_from_query_result(result: &QueryResult) -> Result<Self> {
        Ok(Self {
            entries_total: result
                .try_get_by_index(0)
                .context("failed to read entry count")?,
            first_seen_at_oldest: result
                .try_get_by_index(1)
                .context("failed to read oldest entry first_seen_at")?,
            first_seen_at_newest: result
                .try_get_by_index(2)
                .context("failed to read newest entry first_seen_at")?,
            last_seen_at_oldest: result
                .try_get_by_index(3)
                .context("failed to read oldest entry last_seen_at")?,
            last_seen_at_newest: result
                .try_get_by_index(4)
                .context("failed to read newest entry last_seen_at")?,
            updated_at_oldest: result
                .try_get_by_index(5)
                .context("failed to read oldest entry updated_at")?,
            updated_at_newest: result
                .try_get_by_index(6)
                .context("failed to read newest entry updated_at")?,
            sync_runs_total: result
                .try_get_by_index(7)
                .context("failed to read sync-run count")?,
            sync_runs_success: result
                .try_get_by_index(8)
                .context("failed to read successful sync-run count")?,
            sync_runs_not_modified: result
                .try_get_by_index(9)
                .context("failed to read not-modified sync-run count")?,
            sync_runs_failed: result
                .try_get_by_index(10)
                .context("failed to read failed sync-run count")?,
            sync_runs_running: result
                .try_get_by_index(11)
                .context("failed to read running sync-run count")?,
            records_seen_total: result
                .try_get_by_index(12)
                .context("failed to read records-seen total")?,
            records_inserted_total: result
                .try_get_by_index(13)
                .context("failed to read records-inserted total")?,
            records_updated_total: result
                .try_get_by_index(14)
                .context("failed to read records-updated total")?,
            sync_checked_at_oldest: result
                .try_get_by_index(15)
                .context("failed to read oldest sync checked_at")?,
            sync_checked_at_newest: result
                .try_get_by_index(16)
                .context("failed to read newest sync checked_at")?,
            sync_completed_at_newest: result
                .try_get_by_index(17)
                .context("failed to read newest sync completed_at")?,
        })
    }
}

fn print_stats(summary: &Summary) {
    println!("entries_total: {}", summary.entries_total);
    print_timestamp(
        "entry_first_seen_at_oldest",
        summary.first_seen_at_oldest.as_deref(),
    );
    print_timestamp(
        "entry_first_seen_at_newest",
        summary.first_seen_at_newest.as_deref(),
    );
    print_timestamp(
        "entry_last_seen_at_oldest",
        summary.last_seen_at_oldest.as_deref(),
    );
    print_timestamp(
        "entry_last_seen_at_newest",
        summary.last_seen_at_newest.as_deref(),
    );
    print_timestamp(
        "entry_updated_at_oldest",
        summary.updated_at_oldest.as_deref(),
    );
    print_timestamp(
        "entry_updated_at_newest",
        summary.updated_at_newest.as_deref(),
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
        summary.sync_checked_at_oldest.as_deref(),
    );
    print_timestamp(
        "sync_checked_at_newest",
        summary.sync_checked_at_newest.as_deref(),
    );
    print_timestamp(
        "sync_completed_at_newest",
        summary.sync_completed_at_newest.as_deref(),
    );
}

fn print_timestamp(label: &str, timestamp: Option<&str>) {
    println!("{label}: {}", optional_timestamp(timestamp));
}

fn optional_timestamp(timestamp: Option<&str>) -> &str {
    timestamp.unwrap_or("<none>")
}

#[cfg(test)]
mod tests {
    use super::{command, optional_timestamp};

    #[test]
    fn kev_stats_accepts_no_arguments() {
        command()
            .try_get_matches_from(["stats"])
            .expect("stats should parse");
    }

    #[test]
    fn optional_timestamp_uses_none_marker() {
        assert_eq!(optional_timestamp(None), "<none>");
    }
}
