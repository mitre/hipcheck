use anyhow::{Context as _, Result, bail};
use clap::ValueEnum;
use nv_common::{config::Config, db, rt};
use sea_orm::{ConnectionTrait, DatabaseBackend, QueryResult, Statement, Value};
use serde_json::json;
use std::fmt::Write as _;

const DEFAULT_LIMIT: i64 = 50;
const CISA_KEV_CATALOG_URL: &str = "https://www.cisa.gov/known-exploited-vulnerabilities-catalog";
const CVE_LIST_NEVER_SYNCED_WARNING: &str = "the CVE List has never successfully synced; audit results may be incomplete. \
     Run `nvdb cve sync --destructive`.";
const KEV_NEVER_SYNCED_WARNING: &str = "the KEV catalog has never successfully synced; audit results may be incomplete. \
     Run `nvdb kev sync --destructive`.";

pub fn command() -> clap::Command {
	clap::Command::new("audit")
		.about("Audit reciprocal CVE List and KEV catalog references")
		.arg(
			clap::Arg::new("state")
				.long("state")
				.value_name("STATE")
				.action(clap::ArgAction::Append)
				.value_parser(clap::value_parser!(AuditState))
				.help("List entries in a relationship state; can be repeated"),
		)
		.arg(
			clap::Arg::new("limit")
				.long("limit")
				.conflicts_with("no-limit")
				.value_name("N")
				.default_value(DEFAULT_LIMIT.to_string())
				.value_parser(clap::value_parser!(i64).range(1..))
				.help("Maximum number of matching entries to list"),
		)
		.arg(
			clap::Arg::new("no-limit")
				.long("no-limit")
				.action(clap::ArgAction::SetTrue)
				.help("List all matching entries without a limit"),
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
				.help("Print audit results as JSON"),
		)
		.arg(
			clap::Arg::new("check")
				.long("check")
				.action(clap::ArgAction::SetTrue)
				.help("Exit non-zero when one-sided references exist"),
		)
}

pub fn run(config: &Config, matches: &clap::ArgMatches) -> Result<()> {
	let filter = AuditFilter::from_matches(matches);
	let runtime = rt::AsyncRuntime::new(config).context("failed to create async runtime")?;

	runtime.block_on(audit(config, &filter))
}

async fn audit(config: &Config, filter: &AuditFilter) -> Result<()> {
	let db = db::connection(config)
		.await
		.context("failed to connect to database")?;
	let sync_availability = read_sync_availability(&db).await?;
	let summary = read_summary(&db).await?;
	let entries = if filter.states.is_empty() {
		ListedAuditEntries::default()
	} else {
		read_entries(&db, filter).await?
	};

	print_sync_warnings(&sync_availability);
	if filter.output_json {
		print_json(&sync_availability, &summary, &entries.entries)?;
	} else {
		print_sync_times(&sync_availability);
		print_summary(&summary);
		print_entries(&entries.entries);
	}
	if let Some(limit) = entries.truncated_at {
		print_limit_warning(limit);
	}
	if filter.check && summary.has_one_sided_references() {
		bail!("CVE/KEV audit found one-sided references");
	}

	Ok(())
}

async fn read_sync_availability<C>(db: &C) -> Result<SyncAvailability>
where
	C: ConnectionTrait,
{
	let results = db
		.query_all_raw(sync_availability_statement())
		.await
		.context("failed to read CVE/KEV sync availability")?;
	let result = results
		.first()
		.expect("sync availability query always returns one row");

	Ok(SyncAvailability {
		cve_list_has_successful_sync: result
			.try_get("", "cve_list_has_successful_sync")
			.context("failed to read CVE List sync availability")?,
		kev_has_successful_sync: result
			.try_get("", "kev_has_successful_sync")
			.context("failed to read KEV sync availability")?,
		cve_list_last_successful_sync_at: result
			.try_get("", "cve_list_last_successful_sync_at")
			.context("failed to read CVE List last successful sync time")?,
		kev_last_successful_sync_at: result
			.try_get("", "kev_last_successful_sync_at")
			.context("failed to read KEV last successful sync time")?,
	})
}

async fn read_summary<C>(db: &C) -> Result<AuditSummary>
where
	C: ConnectionTrait,
{
	let results = db
		.query_all_raw(summary_statement())
		.await
		.context("failed to read CVE/KEV audit summary")?;
	AuditSummary::from_query_results(&results)
}

async fn read_entries<C>(db: &C, filter: &AuditFilter) -> Result<ListedAuditEntries>
where
	C: ConnectionTrait,
{
	let results = db
		.query_all_raw(entries_statement(filter))
		.await
		.context("failed to read CVE/KEV audit entries")?;
	let mut entries: Vec<_> = results
		.iter()
		.map(AuditEntry::try_from_query_result)
		.collect::<Result<_>>()?;
	let truncated_at = match filter.limit {
		Some(limit) if entries.len() > usize_limit(limit) => {
			entries.truncate(usize_limit(limit));
			Some(limit)
		}
		_ => None,
	};

	Ok(ListedAuditEntries {
		entries,
		truncated_at,
	})
}

fn summary_statement() -> Statement {
	Statement::from_string(
		DatabaseBackend::Postgres,
		format!(
			"{} SELECT audit_state, COUNT(*)::bigint AS count \
             FROM classified GROUP BY audit_state",
			classified_cte()
		),
	)
}

fn sync_availability_statement() -> Statement {
	Statement::from_string(
		DatabaseBackend::Postgres,
		"SELECT \
             EXISTS(SELECT 1 FROM public.cve_list_sync_runs WHERE status = 'success') \
                 AS cve_list_has_successful_sync, \
             EXISTS(SELECT 1 FROM public.cisa_kev_sync_runs \
                    WHERE status IN ('success', 'not_modified')) \
                 AS kev_has_successful_sync, \
             (SELECT completed_at::text FROM public.cve_list_sync_runs \
              WHERE status = 'success' ORDER BY generation DESC LIMIT 1) \
                 AS cve_list_last_successful_sync_at, \
             (SELECT completed_at::text FROM public.cisa_kev_sync_runs \
              WHERE status IN ('success', 'not_modified') ORDER BY generation DESC LIMIT 1) \
                 AS kev_last_successful_sync_at"
			.to_owned(),
	)
}

fn entries_statement(filter: &AuditFilter) -> Statement {
	let mut sql = format!(
		"{} SELECT cve_id, audit_state, kev_entry_exists, cve_record_exists, \
         cve_references_kev FROM classified WHERE ",
		classified_cte()
	);
	let mut values = Vec::new();
	let states = filter
		.states
		.iter()
		.map(|state| {
			let parameter = next_parameter(&values);
			values.push(Value::String(Some(state.as_str().to_owned())));
			format!("audit_state = ${parameter}")
		})
		.collect::<Vec<_>>();
	write!(sql, "({})", states.join(" OR ")).expect("writing to String should not fail");

	let order = if filter.descending { "DESC" } else { "ASC" };
	write!(sql, " ORDER BY cve_id {order}").expect("writing to String should not fail");
	if let Some(limit) = filter.limit {
		write!(sql, " LIMIT ${}", next_parameter(&values))
			.expect("writing to String should not fail");
		values.push(Value::BigInt(Some(query_limit(limit))));
	}

	Statement::from_sql_and_values(DatabaseBackend::Postgres, sql, values)
}

fn classified_cte() -> String {
	let catalog_url_pattern = CISA_KEV_CATALOG_URL.replace('.', r"\.");

	format!(
		"WITH active_cves AS ( \
             SELECT cve_id, \
                 (EXISTS(SELECT 1 FROM jsonb_path_query(record, \
                    '$.containers.cna.references[*].url') AS cna_reference(url) \
                    WHERE cna_reference.url #>> '{{}}' \
                        ~ E'^{catalog_url_pattern}/?(\\\\?.*)?$') \
                  OR EXISTS(SELECT 1 FROM jsonb_path_query(record, \
                    '$.containers.adp[*].references[*].url') AS adp_reference(url) \
                    WHERE adp_reference.url #>> '{{}}' \
                        ~ E'^{catalog_url_pattern}/?(\\\\?.*)?$')) \
                 AS cve_references_kev \
             FROM public.cve_list_records \
             WHERE deleted = false \
         ), candidates AS ( \
             SELECT cve_id FROM active_cves \
             UNION \
             SELECT cve_id FROM public.cisa_kev_entries WHERE removed_at IS NULL \
         ), classified AS ( \
             SELECT candidates.cve_id, \
                 kev.cve_id IS NOT NULL AS kev_entry_exists, \
                 cve.cve_id IS NOT NULL AS cve_record_exists, \
                 COALESCE(cve.cve_references_kev, false) AS cve_references_kev, \
                 CASE \
                     WHEN kev.cve_id IS NOT NULL AND COALESCE(cve.cve_references_kev, false) THEN 'both' \
                     WHEN kev.cve_id IS NOT NULL THEN 'kev-only' \
                     WHEN COALESCE(cve.cve_references_kev, false) THEN 'cve-only' \
                     ELSE 'neither' \
                 END AS audit_state \
             FROM candidates \
             LEFT JOIN active_cves cve ON cve.cve_id = candidates.cve_id \
             LEFT JOIN public.cisa_kev_entries kev ON kev.cve_id = candidates.cve_id AND kev.removed_at IS NULL \
         )"
	)
}

fn next_parameter(values: &[Value]) -> usize {
	values
		.len()
		.checked_add(1)
		.expect("audit query parameter count should fit in usize")
}

fn query_limit(limit: i64) -> i64 {
	limit
		.checked_add(1)
		.expect("audit query limit should fit in i64")
}

fn usize_limit(limit: i64) -> usize {
	usize::try_from(limit).expect("positive audit limit should fit in usize")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum AuditState {
	Both,
	KevOnly,
	CveOnly,
	Neither,
}

impl AuditState {
	fn as_str(self) -> &'static str {
		match self {
			Self::Both => "both",
			Self::KevOnly => "kev-only",
			Self::CveOnly => "cve-only",
			Self::Neither => "neither",
		}
	}

	#[cfg(test)]
	fn classify(kev_entry_exists: bool, cve_references_kev: bool) -> Self {
		match (kev_entry_exists, cve_references_kev) {
			(true, true) => Self::Both,
			(true, false) => Self::KevOnly,
			(false, true) => Self::CveOnly,
			(false, false) => Self::Neither,
		}
	}

	fn from_database(value: &str) -> Result<Self> {
		Self::value_variants()
			.iter()
			.copied()
			.find(|state| state.as_str() == value)
			.ok_or_else(|| anyhow::anyhow!("invalid CVE/KEV audit state from database: {value}"))
	}
}

#[derive(Debug, PartialEq, Eq)]
struct AuditFilter {
	states: Vec<AuditState>,
	limit: Option<i64>,
	descending: bool,
	output_json: bool,
	check: bool,
}

impl AuditFilter {
	fn from_matches(matches: &clap::ArgMatches) -> Self {
		Self {
			states: matches
				.get_many::<AuditState>("state")
				.map(|states| states.copied().collect())
				.unwrap_or_default(),
			limit: (!matches.get_flag("no-limit")).then(|| {
				*matches
					.get_one::<i64>("limit")
					.expect("limit has a default value")
			}),
			descending: matches.get_flag("desc"),
			output_json: matches.get_flag("json"),
			check: matches.get_flag("check"),
		}
	}
}

#[derive(Debug, Default, PartialEq, Eq)]
struct AuditSummary {
	both: i64,
	kev_only: i64,
	cve_only: i64,
	neither: i64,
}

#[derive(Debug, PartialEq, Eq)]
struct SyncAvailability {
	cve_list_has_successful_sync: bool,
	kev_has_successful_sync: bool,
	cve_list_last_successful_sync_at: Option<String>,
	kev_last_successful_sync_at: Option<String>,
}

impl AuditSummary {
	fn from_query_results(results: &[QueryResult]) -> Result<Self> {
		let mut summary = Self::default();
		for result in results {
			let state: String = result
				.try_get("", "audit_state")
				.context("failed to read audit state")?;
			let count: i64 = result
				.try_get("", "count")
				.context("failed to read audit state count")?;
			match AuditState::from_database(&state)? {
				AuditState::Both => summary.both = count,
				AuditState::KevOnly => summary.kev_only = count,
				AuditState::CveOnly => summary.cve_only = count,
				AuditState::Neither => summary.neither = count,
			}
		}
		Ok(summary)
	}

	fn has_one_sided_references(&self) -> bool {
		self.kev_only != 0 || self.cve_only != 0
	}
}

#[derive(Debug, PartialEq, Eq)]
struct AuditEntry {
	cve_id: String,
	state: AuditState,
	kev_entry_exists: bool,
	cve_record_exists: bool,
	cve_references_kev: bool,
}

impl AuditEntry {
	fn try_from_query_result(result: &QueryResult) -> Result<Self> {
		let state: String = result
			.try_get("", "audit_state")
			.context("failed to read audit entry state")?;
		Ok(Self {
			cve_id: result
				.try_get("", "cve_id")
				.context("failed to read audit entry CVE ID")?,
			state: AuditState::from_database(&state)?,
			kev_entry_exists: result
				.try_get("", "kev_entry_exists")
				.context("failed to read KEV entry presence")?,
			cve_record_exists: result
				.try_get("", "cve_record_exists")
				.context("failed to read CVE record presence")?,
			cve_references_kev: result
				.try_get("", "cve_references_kev")
				.context("failed to read CVE KEV reference presence")?,
		})
	}
}

#[derive(Debug, Default, PartialEq, Eq)]
struct ListedAuditEntries {
	entries: Vec<AuditEntry>,
	truncated_at: Option<i64>,
}

fn print_summary(summary: &AuditSummary) {
	println!("CVE/KEV reference audit");
	println!("both: {}", summary.both);
	println!("kev-only: {}", summary.kev_only);
	println!("cve-only: {}", summary.cve_only);
	println!("neither: {}", summary.neither);
}

fn print_sync_times(sync_availability: &SyncAvailability) {
	println!(
		"cve_list_last_successful_sync_at: {}",
		sync_availability
			.cve_list_last_successful_sync_at
			.as_deref()
			.unwrap_or("<none>")
	);
	println!(
		"kev_last_successful_sync_at: {}",
		sync_availability
			.kev_last_successful_sync_at
			.as_deref()
			.unwrap_or("<none>")
	);
}

fn print_sync_warnings(sync_availability: &SyncAvailability) {
	for warning in sync_warning_messages(sync_availability) {
		eprintln!("warning: {warning}");
	}
}

fn sync_warning_messages(sync_availability: &SyncAvailability) -> Vec<&'static str> {
	let mut warnings = Vec::new();
	if !sync_availability.cve_list_has_successful_sync {
		warnings.push(CVE_LIST_NEVER_SYNCED_WARNING);
	}
	if !sync_availability.kev_has_successful_sync {
		warnings.push(KEV_NEVER_SYNCED_WARNING);
	}
	warnings
}

fn print_entries(entries: &[AuditEntry]) {
	for entry in entries {
		let cve_record = if entry.cve_record_exists {
			"present"
		} else {
			"missing"
		};
		println!(
			"{} state={} cve_record={cve_record}",
			entry.cve_id,
			entry.state.as_str()
		);
	}
}

fn print_json(
	sync_availability: &SyncAvailability,
	summary: &AuditSummary,
	entries: &[AuditEntry],
) -> Result<()> {
	let entries: Vec<_> = entries
		.iter()
		.map(|entry| {
			json!({
				"cve_id": entry.cve_id,
				"state": entry.state.as_str(),
				"kev_entry_exists": entry.kev_entry_exists,
				"cve_record_exists": entry.cve_record_exists,
				"cve_references_kev": entry.cve_references_kev,
			})
		})
		.collect();
	let output = serde_json::to_string_pretty(&json!({
		"sync": {
			"cve_list_last_successful_sync_at": sync_availability.cve_list_last_successful_sync_at,
			"kev_last_successful_sync_at": sync_availability.kev_last_successful_sync_at,
		},
		"summary": {
			"both": summary.both,
			"kev_only": summary.kev_only,
			"cve_only": summary.cve_only,
			"neither": summary.neither,
		},
		"entries": entries,
	}))
	.context("failed to serialize CVE/KEV audit JSON")?;
	println!("{output}");
	Ok(())
}

fn print_limit_warning(limit: i64) {
	eprintln!(
		"warning: more than {limit} CVE/KEV audit entries matched; output was limited to {limit}. \
         To see the full list, run `nvdb kev audit --no-limit`."
	);
}

#[cfg(test)]
mod tests {
	use super::{
		AuditFilter, AuditState, AuditSummary, CVE_LIST_NEVER_SYNCED_WARNING,
		KEV_NEVER_SYNCED_WARNING, SyncAvailability, classified_cte, command, entries_statement,
		sync_availability_statement, sync_warning_messages,
	};

	#[test]
	fn audit_accepts_no_filters() {
		let matches = command()
			.try_get_matches_from(["audit"])
			.expect("audit should parse");

		assert_eq!(
			AuditFilter::from_matches(&matches),
			AuditFilter {
				states: Vec::new(),
				limit: Some(50),
				descending: false,
				output_json: false,
				check: false,
			}
		);
	}

	#[test]
	fn audit_accepts_repeated_states_and_output_options() {
		let matches = command()
			.try_get_matches_from([
				"audit", "--state", "kev-only", "--state", "cve-only", "--limit", "25", "--desc",
				"--json", "--check",
			])
			.expect("audit should parse");

		assert_eq!(
			AuditFilter::from_matches(&matches),
			AuditFilter {
				states: vec![AuditState::KevOnly, AuditState::CveOnly],
				limit: Some(25),
				descending: true,
				output_json: true,
				check: true,
			}
		);
	}

	#[test]
	fn audit_rejects_invalid_state() {
		command()
			.try_get_matches_from(["audit", "--state", "invalid"])
			.expect_err("invalid audit state should be rejected");
	}

	#[test]
	fn audit_rejects_limit_and_no_limit_together() {
		command()
			.try_get_matches_from(["audit", "--limit", "10", "--no-limit"])
			.expect_err("limit and no-limit should conflict");
	}

	#[test]
	fn audit_classifies_all_reference_states() {
		assert_eq!(AuditState::classify(true, true), AuditState::Both);
		assert_eq!(AuditState::classify(true, false), AuditState::KevOnly);
		assert_eq!(AuditState::classify(false, true), AuditState::CveOnly);
		assert_eq!(AuditState::classify(false, false), AuditState::Neither);
	}

	#[test]
	fn audit_cte_checks_cna_and_adp_references_and_excludes_deleted_records() {
		let sql = classified_cte();

		assert!(sql.contains("$.containers.cna.references[*].url"));
		assert!(sql.contains("$.containers.adp[*].references[*].url"));
		assert!(sql.contains(
			"E'^https://www\\.cisa\\.gov/known-exploited-vulnerabilities-catalog/?(\\\\?.*)?$'"
		));
		assert!(sql.contains("WHERE deleted = false"));
	}

	#[test]
	fn audit_entries_query_is_parameterized_and_limited() {
		let statement = entries_statement(&AuditFilter {
			states: vec![AuditState::KevOnly, AuditState::CveOnly],
			limit: Some(50),
			descending: false,
			output_json: false,
			check: false,
		});

		assert!(
			statement
				.sql
				.contains("audit_state = $1 OR audit_state = $2")
		);
		assert!(statement.sql.contains("ORDER BY cve_id ASC LIMIT $3"));
	}

	#[test]
	fn audit_sync_availability_uses_existing_success_criteria() {
		let statement = sync_availability_statement();

		assert!(
			statement
				.sql
				.contains("cve_list_sync_runs WHERE status = 'success'")
		);
		assert!(
			statement
				.sql
				.contains("cisa_kev_sync_runs WHERE status IN ('success', 'not_modified')")
		);
		assert!(statement.sql.contains("cve_list_last_successful_sync_at"));
		assert!(statement.sql.contains("kev_last_successful_sync_at"));
	}

	#[test]
	fn audit_check_only_fails_for_one_sided_references() {
		assert!(
			!AuditSummary {
				both: 1,
				kev_only: 0,
				cve_only: 0,
				neither: 1,
			}
			.has_one_sided_references()
		);
		assert!(
			AuditSummary {
				both: 0,
				kev_only: 1,
				cve_only: 0,
				neither: 0,
			}
			.has_one_sided_references()
		);
	}

	#[test]
	fn audit_warns_when_either_dataset_has_never_synced() {
		let cve_missing = SyncAvailability {
			cve_list_has_successful_sync: false,
			kev_has_successful_sync: true,
			cve_list_last_successful_sync_at: None,
			kev_last_successful_sync_at: Some("2026-08-26T00:00:00+00:00".to_owned()),
		};
		let kev_missing = SyncAvailability {
			cve_list_has_successful_sync: true,
			kev_has_successful_sync: false,
			cve_list_last_successful_sync_at: Some("2026-08-26T00:00:00+00:00".to_owned()),
			kev_last_successful_sync_at: None,
		};

		assert_eq!(
			sync_warning_messages(&cve_missing),
			[CVE_LIST_NEVER_SYNCED_WARNING]
		);
		assert_eq!(
			sync_warning_messages(&kev_missing),
			[KEV_NEVER_SYNCED_WARNING]
		);
	}
}
