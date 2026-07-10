use anyhow::{Context as _, Result, anyhow};
use nv_common::{config::Config, db, rt};
use sea_orm::{ConnectionTrait, DatabaseBackend, QueryResult, Statement};

pub fn command() -> clap::Command {
    clap::Command::new("stats").about("Print stored CVE List record statistics")
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
    let states = read_grouped_counts(
        &db,
        "SELECT \
             COALESCE(NULLIF(record #>> '{cveMetadata,state}', ''), '<unknown>') AS value, \
             COUNT(*) AS count \
         FROM public.cve_list_records \
         GROUP BY 1 \
         ORDER BY 2 DESC, 1 ASC",
        "failed to read CVE state counts",
    )
    .await?;
    let record_format_versions = read_grouped_counts(
        &db,
        "SELECT record_format_version AS value, COUNT(*) AS count \
         FROM public.cve_list_records \
         GROUP BY record_format_version \
         ORDER BY 2 DESC, 1 ASC",
        "failed to read CVE record format version counts",
    )
    .await?;

    print_stats(&summary, &states, &record_format_versions);

    Ok(())
}

async fn read_summary<C>(db: &C) -> Result<Summary>
where
    C: ConnectionTrait,
{
    let statement = Statement::from_string(
        DatabaseBackend::Postgres,
        "SELECT \
             COUNT(*) FILTER (WHERE NOT deleted) AS active_records, \
             COUNT(*) FILTER (WHERE deleted) AS deleted_records, \
             MIN(first_seen_at)::text AS oldest_first_seen_at, \
             MAX(first_seen_at)::text AS newest_first_seen_at, \
             MIN(last_seen_at)::text AS oldest_last_seen_at, \
             MAX(last_seen_at)::text AS newest_last_seen_at, \
             MIN(updated_at)::text AS oldest_updated_at, \
             MAX(updated_at)::text AS newest_updated_at \
         FROM public.cve_list_records"
            .to_owned(),
    );
    let result = db
        .query_one_raw(statement)
        .await
        .context("failed to read CVE List record summary")?
        .ok_or_else(|| anyhow!("CVE List record summary query returned no row"))?;

    Summary::try_from_query_result(&result)
}

async fn read_grouped_counts<C>(
    db: &C,
    sql: &str,
    context: &'static str,
) -> Result<Vec<GroupedCount>>
where
    C: ConnectionTrait,
{
    let statement = Statement::from_string(DatabaseBackend::Postgres, sql.to_owned());
    let results = db.query_all_raw(statement).await.context(context)?;

    results
        .iter()
        .map(GroupedCount::try_from_query_result)
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
struct Summary {
    active_records: i64,
    deleted_records: i64,
    oldest_first_seen_at: Option<String>,
    newest_first_seen_at: Option<String>,
    oldest_last_seen_at: Option<String>,
    newest_last_seen_at: Option<String>,
    oldest_updated_at: Option<String>,
    newest_updated_at: Option<String>,
}

impl Summary {
    fn total_records(&self) -> i64 {
        self.active_records
            .checked_add(self.deleted_records)
            .expect("CVE List record counts should fit in i64")
    }

    fn try_from_query_result(result: &QueryResult) -> Result<Self> {
        Ok(Self {
            active_records: result
                .try_get_by_index(0)
                .context("failed to read active CVE List record count")?,
            deleted_records: result
                .try_get_by_index(1)
                .context("failed to read deleted CVE List record count")?,
            oldest_first_seen_at: result
                .try_get_by_index(2)
                .context("failed to read oldest first_seen_at")?,
            newest_first_seen_at: result
                .try_get_by_index(3)
                .context("failed to read newest first_seen_at")?,
            oldest_last_seen_at: result
                .try_get_by_index(4)
                .context("failed to read oldest last_seen_at")?,
            newest_last_seen_at: result
                .try_get_by_index(5)
                .context("failed to read newest last_seen_at")?,
            oldest_updated_at: result
                .try_get_by_index(6)
                .context("failed to read oldest updated_at")?,
            newest_updated_at: result
                .try_get_by_index(7)
                .context("failed to read newest updated_at")?,
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
struct GroupedCount {
    value: String,
    count: i64,
}

impl GroupedCount {
    fn try_from_query_result(result: &QueryResult) -> Result<Self> {
        Ok(Self {
            value: result
                .try_get_by_index(0)
                .context("failed to read grouped count value")?,
            count: result
                .try_get_by_index(1)
                .context("failed to read grouped count")?,
        })
    }
}

fn print_stats(
    summary: &Summary,
    states: &[GroupedCount],
    record_format_versions: &[GroupedCount],
) {
    println!("records_total: {}", summary.total_records());
    println!("records_active: {}", summary.active_records);
    println!("records_deleted: {}", summary.deleted_records);
    println!(
        "first_seen_at_oldest: {}",
        optional_timestamp(summary.oldest_first_seen_at.as_deref())
    );
    println!(
        "first_seen_at_newest: {}",
        optional_timestamp(summary.newest_first_seen_at.as_deref())
    );
    println!(
        "last_seen_at_oldest: {}",
        optional_timestamp(summary.oldest_last_seen_at.as_deref())
    );
    println!(
        "last_seen_at_newest: {}",
        optional_timestamp(summary.newest_last_seen_at.as_deref())
    );
    println!(
        "updated_at_oldest: {}",
        optional_timestamp(summary.oldest_updated_at.as_deref())
    );
    println!(
        "updated_at_newest: {}",
        optional_timestamp(summary.newest_updated_at.as_deref())
    );
    print_grouped_counts("cve_states", states);
    print_grouped_counts("record_format_versions", record_format_versions);
}

fn optional_timestamp(value: Option<&str>) -> &str {
    value.unwrap_or("<none>")
}

fn print_grouped_counts(label: &str, counts: &[GroupedCount]) {
    println!("{label}:");

    if counts.is_empty() {
        println!("  <none>: 0");
        return;
    }

    for count in counts {
        println!("  {}: {}", count.value, count.count);
    }
}

#[cfg(test)]
mod tests {
    use super::{GroupedCount, Summary, command, optional_timestamp};

    #[test]
    fn cve_stats_accepts_no_arguments() {
        command()
            .try_get_matches_from(["stats"])
            .expect("stats should parse");
    }

    #[test]
    fn summary_total_records_adds_active_and_deleted_records() {
        let summary = Summary {
            active_records: 3,
            deleted_records: 2,
            oldest_first_seen_at: None,
            newest_first_seen_at: None,
            oldest_last_seen_at: None,
            newest_last_seen_at: None,
            oldest_updated_at: None,
            newest_updated_at: None,
        };

        assert_eq!(summary.total_records(), 5);
    }

    #[test]
    fn optional_timestamp_prints_none_marker() {
        assert_eq!(optional_timestamp(None), "<none>");
        assert_eq!(
            optional_timestamp(Some("2026-07-10 12:00:00+00")),
            "2026-07-10 12:00:00+00"
        );
    }

    #[test]
    fn grouped_count_keeps_label_and_count() {
        let count = GroupedCount {
            value: "PUBLISHED".to_owned(),
            count: 12,
        };

        assert_eq!(count.value, "PUBLISHED");
        assert_eq!(count.count, 12);
    }
}
