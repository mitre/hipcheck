use anyhow::{Context as _, Result};
use nv_common::{config::Config, db, rt};
use sea_orm::{ConnectionTrait, DatabaseBackend, QueryResult, Statement, Value};
use serde_json::json;
use std::fmt::Write as _;

const DEFAULT_LIMIT: i64 = 50;

pub fn command() -> clap::Command {
    clap::Command::new("list")
        .about("List stored CVE IDs")
        .arg(
            clap::Arg::new("state")
                .long("state")
                .value_name("STATE")
                .action(clap::ArgAction::Append)
                .help("Filter by CVE record state; can be repeated"),
        )
        .arg(
            clap::Arg::new("year")
                .long("year")
                .value_name("YYYY")
                .value_parser(clap::value_parser!(u16).range(1000..=9999))
                .help("Filter by CVE year"),
        )
        .arg(
            clap::Arg::new("active")
                .long("active")
                .conflicts_with("deleted")
                .action(clap::ArgAction::SetTrue)
                .help("Only list active records"),
        )
        .arg(
            clap::Arg::new("deleted")
                .long("deleted")
                .conflicts_with("active")
                .action(clap::ArgAction::SetTrue)
                .help("Only list deleted records"),
        )
        .arg(
            clap::Arg::new("seen-since")
                .long("seen-since")
                .value_name("TIMESTAMP")
                .help("Only list records last seen at or after this timestamp")
                .long_help(
                    "Only list records last seen at or after this PostgreSQL timestamptz. \
                     RFC 3339 is recommended, e.g. 2026-07-01T00:00:00Z.",
                ),
        )
        .arg(
            clap::Arg::new("updated-since")
                .long("updated-since")
                .value_name("TIMESTAMP")
                .help("Only list records updated at or after this timestamp")
                .long_help(
                    "Only list records updated at or after this PostgreSQL timestamptz. \
                     RFC 3339 is recommended, e.g. 2026-07-01T00:00:00Z.",
                ),
        )
        .arg(
            clap::Arg::new("limit")
                .long("limit")
                .conflicts_with("no-limit")
                .value_name("N")
                .default_value(DEFAULT_LIMIT.to_string())
                .value_parser(clap::value_parser!(i64).range(1..))
                .help("Maximum number of CVE records to list"),
        )
        .arg(
            clap::Arg::new("no-limit")
                .long("no-limit")
                .action(clap::ArgAction::SetTrue)
                .help("List all matching CVE records without a limit"),
        )
        .arg(
            clap::Arg::new("desc")
                .long("desc")
                .action(clap::ArgAction::SetTrue)
                .help("List CVE IDs in descending order"),
        )
        .arg(
            clap::Arg::new("json")
                .long("json")
                .action(clap::ArgAction::SetTrue)
                .help("Print matching CVE metadata as JSON"),
        )
}

pub fn run(config: &Config, matches: &clap::ArgMatches) -> Result<()> {
    let filter = CveListFilter::from_matches(matches);
    let runtime = rt::AsyncRuntime::new(config).context("failed to create async runtime")?;

    runtime.block_on(list(config, &filter))
}

async fn list(config: &Config, filter: &CveListFilter) -> Result<()> {
    let db = db::connection(config)
        .await
        .context("failed to connect to database")?;
    let records = read_records(&db, filter).await?;

    if filter.output_json {
        print_json(&records.records)?;
    } else {
        print_records(&records.records);
    }

    if let Some(limit) = records.truncated_at {
        print_limit_warning(limit);
    }

    Ok(())
}

async fn read_records<C>(db: &C, filter: &CveListFilter) -> Result<ListedCveRecords>
where
    C: ConnectionTrait,
{
    let statement = build_statement(filter);
    let results = db
        .query_all_raw(statement)
        .await
        .context("failed to read CVE List records")?;

    let mut records: Vec<CveListRecordSummary> = results
        .iter()
        .map(CveListRecordSummary::try_from_query_result)
        .collect::<Result<_>>()?;
    let truncated_at = match filter.limit {
        Some(limit) if records.len() > usize_limit(limit) => {
            records.truncate(usize_limit(limit));
            Some(limit)
        }
        _ => None,
    };

    Ok(ListedCveRecords {
        records,
        truncated_at,
    })
}

fn build_statement(filter: &CveListFilter) -> Statement {
    let mut sql = "\
        SELECT \
            cve_id, \
            COALESCE(NULLIF(record #>> '{cveMetadata,state}', ''), '<unknown>') AS state, \
            deleted, \
            first_seen_at::text, \
            last_seen_at::text, \
            updated_at::text \
        FROM public.cve_list_records"
        .to_owned();
    let mut predicates = Vec::new();
    let mut values = Vec::new();

    if !filter.states.is_empty() {
        let mut state_predicates = Vec::new();

        for state in &filter.states {
            state_predicates.push(format!(
                "record #>> '{{cveMetadata,state}}' = ${}",
                next_parameter(&values)
            ));
            values.push(Value::String(Some(state.clone())));
        }

        predicates.push(format!("({})", state_predicates.join(" OR ")));
    }

    if let Some(year) = filter.year {
        predicates.push(format!("cve_id LIKE ${}", next_parameter(&values)));
        values.push(Value::String(Some(format!("CVE-{year:04}-%"))));
    }

    if let Some(deleted) = filter.deleted {
        predicates.push(format!("deleted = ${}", next_parameter(&values)));
        values.push(Value::Bool(Some(deleted)));
    }

    if let Some(seen_since) = &filter.seen_since {
        predicates.push(format!(
            "last_seen_at >= ${}::timestamptz",
            next_parameter(&values)
        ));
        values.push(Value::String(Some(seen_since.clone())));
    }

    if let Some(updated_since) = &filter.updated_since {
        predicates.push(format!(
            "updated_at >= ${}::timestamptz",
            next_parameter(&values)
        ));
        values.push(Value::String(Some(updated_since.clone())));
    }

    if !predicates.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&predicates.join(" AND "));
    }

    let order = if filter.descending { "DESC" } else { "ASC" };
    write!(sql, " ORDER BY cve_id {order}").expect("writing to String should not fail");

    if let Some(limit) = filter.limit {
        write!(sql, " LIMIT ${}", next_parameter(&values))
            .expect("writing to String should not fail");
        values.push(Value::BigInt(Some(query_limit(limit))));
    }

    Statement::from_sql_and_values(DatabaseBackend::Postgres, sql, values)
}

fn next_parameter(values: &[Value]) -> usize {
    values
        .len()
        .checked_add(1)
        .expect("CVE list query parameter count should fit in usize")
}

fn query_limit(limit: i64) -> i64 {
    limit
        .checked_add(1)
        .expect("CVE list query limit should fit in i64")
}

fn usize_limit(limit: i64) -> usize {
    usize::try_from(limit).expect("positive CVE list limit should fit in usize")
}

#[derive(Debug, PartialEq, Eq)]
struct CveListFilter {
    states: Vec<String>,
    year: Option<u16>,
    deleted: Option<bool>,
    seen_since: Option<String>,
    updated_since: Option<String>,
    limit: Option<i64>,
    descending: bool,
    output_json: bool,
}

impl CveListFilter {
    fn from_matches(matches: &clap::ArgMatches) -> Self {
        let states = matches
            .get_many::<String>("state")
            .map(|states| states.cloned().collect())
            .unwrap_or_default();
        let deleted = if matches.get_flag("active") {
            Some(false)
        } else if matches.get_flag("deleted") {
            Some(true)
        } else {
            None
        };

        Self {
            states,
            year: matches.get_one::<u16>("year").copied(),
            deleted,
            seen_since: matches.get_one::<String>("seen-since").cloned(),
            updated_since: matches.get_one::<String>("updated-since").cloned(),
            limit: if matches.get_flag("no-limit") {
                None
            } else {
                Some(
                    *matches
                        .get_one::<i64>("limit")
                        .expect("limit has a default value"),
                )
            },
            descending: matches.get_flag("desc"),
            output_json: matches.get_flag("json"),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ListedCveRecords {
    records: Vec<CveListRecordSummary>,
    truncated_at: Option<i64>,
}

#[derive(Debug, PartialEq, Eq)]
struct CveListRecordSummary {
    cve_id: String,
    state: String,
    deleted: bool,
    first_seen_at: String,
    last_seen_at: String,
    updated_at: String,
}

impl CveListRecordSummary {
    fn try_from_query_result(result: &QueryResult) -> Result<Self> {
        Ok(Self {
            cve_id: result
                .try_get_by_index(0)
                .context("failed to read CVE ID")?,
            state: result
                .try_get_by_index(1)
                .context("failed to read CVE state")?,
            deleted: result
                .try_get_by_index(2)
                .context("failed to read deleted flag")?,
            first_seen_at: result
                .try_get_by_index(3)
                .context("failed to read first_seen_at")?,
            last_seen_at: result
                .try_get_by_index(4)
                .context("failed to read last_seen_at")?,
            updated_at: result
                .try_get_by_index(5)
                .context("failed to read updated_at")?,
        })
    }
}

fn print_records(records: &[CveListRecordSummary]) {
    if records.is_empty() {
        println!("cve_records: <none>");
        return;
    }

    for record in records {
        println!("{}", record.cve_id);
    }
}

fn print_json(records: &[CveListRecordSummary]) -> Result<()> {
    let records: Vec<_> = records
        .iter()
        .map(|record| {
            json!({
                "cve_id": record.cve_id,
                "state": record.state,
                "deleted": record.deleted,
                "first_seen_at": record.first_seen_at,
                "last_seen_at": record.last_seen_at,
                "updated_at": record.updated_at,
            })
        })
        .collect();
    let output = serde_json::to_string_pretty(&records)
        .context("failed to serialize CVE List records JSON")?;

    println!("{output}");
    Ok(())
}

fn print_limit_warning(limit: i64) {
    eprintln!(
        "warning: more than {limit} CVE records matched; output was limited to {limit}. \
         To see the full list with no limit, run `nvdb cve list --no-limit` or \
         `nvdb cve list --no-limit --json`."
    );
}

#[cfg(test)]
mod tests {
    use super::{CveListFilter, build_statement, command};

    #[test]
    fn cve_list_accepts_no_filters() {
        let matches = command()
            .try_get_matches_from(["list"])
            .expect("list should parse");

        assert_eq!(
            CveListFilter::from_matches(&matches),
            CveListFilter {
                states: Vec::new(),
                year: None,
                deleted: None,
                seen_since: None,
                updated_since: None,
                limit: Some(50),
                descending: false,
                output_json: false,
            }
        );
    }

    #[test]
    fn cve_list_accepts_filters() {
        let matches = command()
            .try_get_matches_from([
                "list",
                "--state",
                "PUBLISHED",
                "--state",
                "REJECTED",
                "--year",
                "2026",
                "--active",
                "--seen-since",
                "2026-07-01T00:00:00Z",
                "--updated-since",
                "2026-07-02T00:00:00Z",
                "--limit",
                "25",
                "--desc",
                "--json",
            ])
            .expect("list should parse");

        assert_eq!(
            CveListFilter::from_matches(&matches),
            CveListFilter {
                states: vec!["PUBLISHED".to_owned(), "REJECTED".to_owned()],
                year: Some(2026),
                deleted: Some(false),
                seen_since: Some("2026-07-01T00:00:00Z".to_owned()),
                updated_since: Some("2026-07-02T00:00:00Z".to_owned()),
                limit: Some(25),
                descending: true,
                output_json: true,
            }
        );
    }

    #[test]
    fn cve_list_accepts_no_limit() {
        let matches = command()
            .try_get_matches_from(["list", "--no-limit"])
            .expect("list should parse");

        assert_eq!(CveListFilter::from_matches(&matches).limit, None);
    }

    #[test]
    fn cve_list_rejects_limit_and_no_limit_together() {
        command()
            .try_get_matches_from(["list", "--limit", "10", "--no-limit"])
            .expect_err("limit and no-limit should conflict");
    }

    #[test]
    fn cve_list_rejects_active_and_deleted_together() {
        command()
            .try_get_matches_from(["list", "--active", "--deleted"])
            .expect_err("active and deleted should conflict");
    }

    #[test]
    fn cve_list_rejects_short_years() {
        command()
            .try_get_matches_from(["list", "--year", "26"])
            .expect_err("year should be four digits");
    }

    #[test]
    fn cve_list_sorts_ascending_by_default() {
        let statement = build_statement(&CveListFilter {
            states: Vec::new(),
            year: None,
            deleted: None,
            seen_since: None,
            updated_since: None,
            limit: Some(50),
            descending: false,
            output_json: false,
        });

        assert!(statement.sql.contains("ORDER BY cve_id ASC"));
    }

    #[test]
    fn cve_list_sorts_descending_when_requested() {
        let statement = build_statement(&CveListFilter {
            states: Vec::new(),
            year: None,
            deleted: None,
            seen_since: None,
            updated_since: None,
            limit: Some(50),
            descending: true,
            output_json: false,
        });

        assert!(statement.sql.contains("ORDER BY cve_id DESC"));
    }

    #[test]
    fn cve_list_no_limit_omits_sql_limit() {
        let statement = build_statement(&CveListFilter {
            states: Vec::new(),
            year: None,
            deleted: None,
            seen_since: None,
            updated_since: None,
            limit: None,
            descending: false,
            output_json: false,
        });

        assert!(!statement.sql.contains(" LIMIT "));
    }
}
