use crate::config::Config;
use crate::db::DatabaseConnectionError;
use crate::db::entities::cisa_kev_entries;
use crate::db::entities::cisa_kev_sync_runs;
use crate::error::ErrorSourceIterator as _;
use jiff::civil::Date;
use reqwest::header::{
    ETAG, HeaderMap, HeaderValue, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED,
};
use reqwest::{Client, Response, StatusCode, Url};
use sea_orm::{
    ActiveModelTrait as _, ColumnTrait as _, ConnectionTrait, EntityTrait as _, QueryFilter as _,
    QueryOrder as _, TransactionTrait as _,
};
use sea_orm::{DatabaseBackend, DatabaseConnection, DatabaseTransaction, DbErr, Set, Statement};
use serde::Deserialize;
use slog::{Logger, debug, error, info};
use std::{
    borrow::ToOwned,
    error::Error as _,
    fmt::{Debug, Display, Write as _},
    time::Duration,
};
use tokio::time::sleep;

pub const DEFAULT_KEV_URL: &str =
    "https://www.cisa.gov/sites/default/files/feeds/known_exploited_vulnerabilities.json";
const DEFAULT_KEV_REFRESH_INTERVAL_MILLISECONDS: u64 = 3_600_000;

/// PostgreSQL advisory lock key used to serialize KEV sync work.
/// ASCII text: "KEV_SYNC"
const KEV_SYNC_ADVISORY_LOCK_ID: i64 = 0x4b45_565f_5359_4e43;

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct KevCatalog {
    #[doc = "Version of the known exploited vulnerabilities catalog"]
    pub catalog_version: String,
    #[doc = "Total number of Known Exploited Vulnerabilities in the catalog"]
    pub count: i64,
    #[doc = "Date-time of Catalog Release in the format YYYY-MM-DDTHH:mm:ss.sssZ"]
    // Known issue: this timestamp doesn't directly parse into a `jiff::Zoned`,
    // because that type expects a bracketed named time zone instead of the `Z` character.
    pub date_released: String,
    #[doc = "The exploited vulnerabilities included in this catalog"]
    // Semantically, each entry in this list is of type `Vulnerability`.
    // But we need to store the original JSON for each entry in the database,
    // so we defer parsing them as `Vulnerability` until we insert them.
    // This has the additional benefit of isolating parse errors to be one entry at a time.
    pub vulnerabilities: Vec<serde_json::Value>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Vulnerability {
    #[doc = "The CVE ID of the vulnerability in the format CVE-YYYY-NNNN, note that the number portion can have more than 4 digits"]
    #[serde(rename = "cveID")]
    pub cve_id: String,
    #[doc = "Common Weakness Enumeration (CWE) codes associated with this vulnerability. CWEs are in the format CWE-NNNN; note that the number portion can have any number of digits"]
    #[serde(default)]
    pub cwes: Vec<String>,
    #[doc = "The date the vulnerability was added to the catalog in the format YYYY-MM-DD"]
    pub date_added: Date,
    #[doc = "The date the required action is due in the format YYYY-MM-DD"]
    pub due_date: Date,
    #[doc = "'Known' if this vulnerability is known to have been leveraged as part of a ransomware campaign; 'Unknown' if CISA lacks confirmation that the vulnerability has been utilized for ransomware"]
    #[serde(default)]
    pub known_ransomware_campaign_use: Option<String>,
    #[doc = "Any additional notes about the vulnerability"]
    #[serde(default)]
    pub notes: Option<String>,
    #[doc = "The vulnerability product"]
    pub product: String,
    #[doc = "The required action to address the vulnerability"]
    pub required_action: String,
    #[doc = "A short description of the vulnerability"]
    pub short_description: String,
    #[doc = "The vendor or project name for the vulnerability"]
    pub vendor_project: String,
    #[doc = "The name of the vulnerability"]
    pub vulnerability_name: String,
}

pub enum KevError {
    DatabaseConnectionError(DatabaseConnectionError),
    InvalidCatalogEntry(serde_json::Error),
    ReqwestError(reqwest::Error),
    SeaOrmError(sea_orm::DbErr),
    TransactionLockError,
}

impl From<DatabaseConnectionError> for KevError {
    fn from(error: DatabaseConnectionError) -> Self {
        Self::DatabaseConnectionError(error)
    }
}

impl From<reqwest::Error> for KevError {
    fn from(error: reqwest::Error) -> Self {
        Self::ReqwestError(error)
    }
}

impl From<sea_orm::DbErr> for KevError {
    fn from(error: sea_orm::DbErr) -> Self {
        Self::SeaOrmError(error)
    }
}

// The `Debug` representation for `KevError` is intended to match the
// debug printing for `anyhow::Error`, with a top-level error and then
// a series of causes.
impl Debug for KevError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut msg = format!("{self}\n");

        // If there are causes, then print the "caused by" section.
        if let Some(source) = self.source() {
            msg.push_str("\nCaused by:\n");

            for source in source.sources_iter() {
                let _ = writeln!(msg, "\t{source}");
            }
        }

        write!(f, "{msg}")
    }
}

impl Display for KevError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DatabaseConnectionError(_) => {
                write!(f, "failed to connect to database")
            }
            Self::InvalidCatalogEntry(_) => {
                write!(f, "failed to parse KEV catalog entry")
            }
            Self::ReqwestError(_) => {
                write!(f, "failed to fetch KEV data")
            }
            Self::SeaOrmError(_) => {
                write!(f, "failed to access database")
            }
            Self::TransactionLockError => {
                write!(f, "failed to acquire exclusive database lock to sync data")
            }
        }
    }
}

impl std::error::Error for KevError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::DatabaseConnectionError(err) => Some(err),
            Self::InvalidCatalogEntry(err) => Some(err),
            Self::ReqwestError(err) => Some(err),
            Self::SeaOrmError(err) => Some(err),
            Self::TransactionLockError => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestMode {
    NoConditionalRequest,
    UseConditionalRequest,
}

#[derive(Debug, Clone)]
struct KevConfig {
    kev_refresh_interval: jiff::Span,
    kev_url: Option<reqwest::Url>,
}

impl KevConfig {
    fn from_config(config: &Config) -> Self {
        let refresh_interval_milliseconds = config
            .kev_refresh_interval
            .unwrap_or(DEFAULT_KEV_REFRESH_INTERVAL_MILLISECONDS);
        Self {
            kev_refresh_interval: kev_refresh_interval_span(refresh_interval_milliseconds),
            kev_url: config.kev_url.clone(),
        }
    }
}

fn kev_refresh_interval_span(milliseconds: u64) -> jiff::Span {
    let milliseconds = i64::try_from(milliseconds)
        .expect("KEV refresh interval was validated while parsing configuration");
    jiff::Span::new()
        .try_milliseconds(milliseconds)
        .expect("KEV refresh interval was validated while parsing configuration")
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CacheMetadata {
    etag: Option<String>,
    last_modified: Option<String>,
}

struct RunSummary {
    // TODO
}

pub fn spawn_kev_sync_worker(
    config: &Config,
    db: DatabaseConnection,
    log: Logger,
) -> tokio::task::JoinHandle<()> {
    let kev_config = KevConfig::from_config(config);
    tokio::spawn(loop_fetch_kev(kev_config, db, log))
}

/// Cancellation: TODO
async fn loop_fetch_kev(config: KevConfig, db: DatabaseConnection, log: Logger) {
    info!(log, "started KEV sync worker");

    let interval = Duration::try_from(config.kev_refresh_interval)
        .expect("KEV refresh interval was constructed from validated milliseconds");

    loop {
        let res = sync_kev(
            RequestMode::UseConditionalRequest,
            &config,
            &db,
            log.clone(),
        )
        .await;
        if let Err(e) = res {
            error!(log, "KEV Sync run failed: {e:?}");
        }
        sleep(interval).await;
    }
}

/// This function provides a way for `nvdb` to start a KEV sync run.
/// Cancellation: TODO
pub async fn run_fetch_kev(
    request_mode: RequestMode,
    config: &Config,
    log: Logger,
) -> Result<(), KevError> {
    let db = crate::db::connection(config).await?;
    let kev_config = KevConfig::from_config(config);
    sync_kev(request_mode, &kev_config, &db, log).await
}

/// Cancellation: TODO
async fn sync_kev(
    request_mode: RequestMode,
    config: &KevConfig,
    db: &DatabaseConnection,
    log: Logger,
) -> Result<(), KevError> {
    let xact = acquire_kev_sync_lock(db).await?;
    let sync_run = create_sync_run(&xact).await?;
    // NOTE the clone of sync_run. We want to avoid the possibility of saving
    // an old version of the sync run. We also want to be able to pass it into `fetch_kev`.
    // Assumption: the result of `create_sync_run` is an ActiveModel with all fields set to
    // NotChanged.
    // If that's true, then reusing the ActiveModel should be safe.
    let res = fetch_kev(request_mode, config, sync_run.clone(), &xact, log.clone()).await;
    match &res {
        Ok(()) => update_sync_run_result_success(sync_run, &xact, log).await?,
        Err(e) => {
            let error = format!("{e:?}");
            update_sync_run_result_failure(sync_run, error, &xact, log).await?;
        }
    }
    // Must finish the transaction first, regardless of whether there was an error
    // during processing.
    finish_locked_kev_sync(xact).await?;
    res
}

/// TODO: name this function better
/// Cancellation: TODO
async fn fetch_kev(
    request_mode: RequestMode,
    config: &KevConfig,
    sync_run: cisa_kev_sync_runs::ActiveModel,
    xact: &DatabaseTransaction,
    log: Logger,
) -> Result<(), KevError> {
    // If using a conditional request, check database for previous sync run,
    // to retrieve cache metadata.
    let maybe_run = match request_mode {
        RequestMode::NoConditionalRequest => None,
        RequestMode::UseConditionalRequest => {
            debug!(log, "KEV Sync: Checking for previous sync run");
            get_latest_run(xact).await?
        }
    };

    // If a previous sync run was found, generate appropriate HTTP request headers
    // for a conditional request.
    let previous_cache_metadata = maybe_run.as_ref().map(extract_cache_metadata_from_model);
    let headers = previous_cache_metadata
        .as_ref()
        .map_or_else(HeaderMap::new, |metadata| {
            debug!(log, "KEV Sync: Found previous cache metadata: {metadata:?}");
            generate_cache_headers(metadata)
        });

    let client = Client::new();
    let url = config
        .kev_url
        .clone()
        .unwrap_or(Url::parse(DEFAULT_KEV_URL).expect("default URL should be valid"));
    let request = client.get(url.clone()).headers(headers);
    debug!(log, "KEV Sync: Sending request for KEV Catalog";
        "url" => ?url,
        "request" => ?request);

    // TODO
    // save expires and cache-control from response
    // cache-control max-age directive provides number of seconds until server
    // considers it stale; use that as time to refresh
    let response = request.send().await?;
    debug!(log, "KEV Sync: Got KEV Catalog response"; "response" => ?response);

    let status = response.status();
    let cache_metadata = cache_metadata_for_response(
        status,
        previous_cache_metadata.as_ref(),
        extract_cache_metadata_from_headers(&response),
    );
    debug!(log, "KEV Sync: cache metadata: {cache_metadata:?}");

    // Detect 304 Not Modified and exit early
    if status == StatusCode::NOT_MODIFIED {
        let _sync_run =
            update_sync_run_cache_metadata(cache_metadata, sync_run, xact, log.clone()).await?;
        info!(
            log,
            "KEV Catalog Response was 304 Not Modified; nothing to process"
        );
        return Ok(());
    }

    let catalog = response.json::<KevCatalog>().await?;

    let catalog_version = &catalog.catalog_version;
    let count = catalog.count;
    let date_released = &catalog.date_released;
    debug!(log, "KEV Catalog received";
    "catalog_version" => catalog_version,
    "count" => count,
    "date_released" => date_released
    );

    let _summary = store_all_entries(&catalog.vulnerabilities, xact, log.clone()).await?;
    //let sync_run = update_sync_run_summary(summary, sync_run, xact, log.clone()).await?;

    // Only save response validators after every catalog entry has been stored.
    // Otherwise a later conditional request could hide an entry that failed validation.
    let _sync_run = update_sync_run_cache_metadata(cache_metadata, sync_run, xact, log).await?;

    Ok(())
}

/// This function provides a way for `nvdb` to check KEV status.
/// Cancellation: TODO
pub async fn read_status(config: &Config, log: Logger) -> Result<(), KevError> {
    let db = crate::db::connection(config).await?;
    // TODO is there a more efficient way of just getting the count?
    let res: Vec<cisa_kev_entries::Model> = cisa_kev_entries::Entity::find().all(&db).await?;
    let count = res.len();
    info!(log, "KEV Sync DB contains {count} entries");

    if let Some(entry) = res.first() {
        debug!(log, "first entry: {entry:?}");
    }

    // TODO is there a more efficient way of just getting the count?
    let res: Vec<cisa_kev_sync_runs::Model> = cisa_kev_sync_runs::Entity::find()
        .order_by_desc(cisa_kev_sync_runs::Column::Generation)
        .all(&db)
        .await?;
    let count = res.len();
    info!(log, "KEV Sync DB contains {count} sync runs");

    if let Some(run) = res.first() {
        debug!(log, "most recent run: {run:?}");
        let cache_metadata = extract_cache_metadata_from_model(run);
        debug!(log, "cache metadata: {cache_metadata:?}");
    }

    Ok(())
}

fn convert_one_entry(
    value: &serde_json::Value,
) -> Result<cisa_kev_entries::ActiveModel, serde_json::Error> {
    let vulnerability: Vulnerability = serde_json::from_value(value.clone())?;
    let cve_id = vulnerability.cve_id;

    Ok(cisa_kev_entries::ActiveModel {
        cve_id: Set(cve_id),
        entry: Set(value.clone()),
        ..Default::default()
    })
}

/// Cancellation: TODO
async fn store_all_entries<C>(
    values: &[serde_json::Value],
    db: &C,
    log: Logger,
) -> Result<RunSummary, KevError>
where
    C: ConnectionTrait,
{
    let models = values
        .iter()
        .map(convert_one_entry)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            error!(
                log,
                "KEV Sync: failed to parse JSON as Vulnerability struct, caused by:\n\t{error}"
            );
            KevError::InvalidCatalogEntry(error)
        })?;

    let res = cisa_kev_entries::Entity::insert_many(models)
        .on_conflict(
            sea_query::OnConflict::column(cisa_kev_entries::Column::CveId)
                .update_columns([cisa_kev_entries::Column::Entry])
                .values([
                    (
                        cisa_kev_entries::Column::LastSeenAt,
                        sea_query::Expr::current_timestamp(),
                    ),
                    (
                        cisa_kev_entries::Column::UpdatedAt,
                        sea_query::Expr::cust(
                            "CASE WHEN cisa_kev_entries.entry IS DISTINCT FROM EXCLUDED.entry \
                             THEN CURRENT_TIMESTAMP ELSE cisa_kev_entries.updated_at END",
                        ),
                    ),
                ])
                .to_owned(),
        )
        .exec(db)
        .await;
    match res {
        // Ignore SeaORM errors that say no records were updated or inserted.
        // That is not an error in this case.
        Err(DbErr::RecordNotInserted) => {}
        Err(DbErr::RecordNotUpdated) => {}
        Err(e) => return Err(KevError::SeaOrmError(e)),
        Ok(_) => {}
    }
    debug!(log, "KEV Sync: insert_many result: {res:?}");

    // TODO how to get information on how many entries were inserted/updated?
    let summary = RunSummary {};
    Ok(summary)
}

/// Cancellation: TODO
async fn get_latest_run(
    xact: &DatabaseTransaction,
) -> Result<Option<cisa_kev_sync_runs::Model>, KevError> {
    let run: Option<cisa_kev_sync_runs::Model> = cisa_kev_sync_runs::Entity::find()
        .filter(cisa_kev_sync_runs::Column::Status.eq("success"))
        .order_by_desc(cisa_kev_sync_runs::Column::Generation)
        .one(xact)
        .await?;
    Ok(run)
}

/// Cancellation: TODO
async fn create_sync_run(
    xact: &DatabaseTransaction,
) -> Result<cisa_kev_sync_runs::ActiveModel, KevError> {
    let run = cisa_kev_sync_runs::ActiveModel {
        status: Set("running".to_owned()),
        ..Default::default()
    };
    let stored_run = run.insert(xact).await?;
    Ok(stored_run.into())
}

/// Cancellation: TODO
async fn update_sync_run_cache_metadata(
    cache_metadata: CacheMetadata,
    mut sync_run: cisa_kev_sync_runs::ActiveModel,
    xact: &DatabaseTransaction,
    _log: Logger,
) -> Result<cisa_kev_sync_runs::ActiveModel, KevError> {
    sync_run.etag = Set(cache_metadata.etag);
    sync_run.last_modified = Set(cache_metadata.last_modified);
    let run = sync_run.save(xact).await?;
    Ok(run)
}

/*
/// Cancellation: TODO
async fn update_sync_run_summary(summary: RunSummary, mut sync_run: cisa_kev_sync_runs::ActiveModel, xact: &DatabaseTransaction, log: Logger)
-> Result<cisa_kev_sync_runs::ActiveModel, KevError> {
    // TODO update stats on number of entries processed, updated, etc
    let _run = sync_run.save(xact).await?;
    Ok(run)
}
*/

/// Cancellation: TODO
async fn update_sync_run_result_success(
    mut sync_run: cisa_kev_sync_runs::ActiveModel,
    xact: &DatabaseTransaction,
    _log: Logger,
) -> Result<(), KevError> {
    sync_run.status = Set("success".to_owned());
    let _run = sync_run.save(xact).await?;
    Ok(())
}

/// Cancellation: TODO
async fn update_sync_run_result_failure(
    mut sync_run: cisa_kev_sync_runs::ActiveModel,
    error: String,
    xact: &DatabaseTransaction,
    _log: Logger,
) -> Result<(), KevError> {
    sync_run.status = Set("failed".to_owned());
    sync_run.error = Set(Some(error));
    let _run = sync_run.save(xact).await?;
    Ok(())
}

/// Start a database transaction, and then attempt to acquire the advisory lock.
/// If acquiring the lock is successful, return the transaction.
async fn acquire_kev_sync_lock(db: &DatabaseConnection) -> Result<DatabaseTransaction, KevError> {
    let transaction = db.begin().await?;
    let acquired = try_acquire_kev_sync_lock(&transaction).await?;

    if acquired {
        Ok(transaction)
    } else {
        Err(KevError::TransactionLockError)
    }
}

async fn finish_locked_kev_sync(xact: DatabaseTransaction) -> Result<(), KevError> {
    xact.commit().await?;
    Ok(())
}

/// Attempt to acquire a PostgreSQL advisory lock using raw SQL.
/// The bool result indicates whether the lock was actually acquired.
/// By using the SQL function `pg_try_advisory_xact_lock`, this will return
/// immediately with a result instead of waiting indefinitely for the lock.
async fn try_acquire_kev_sync_lock(db: &DatabaseTransaction) -> Result<bool, KevError> {
    let statement = Statement::from_string(
        DatabaseBackend::Postgres,
        format!("SELECT pg_try_advisory_xact_lock({KEV_SYNC_ADVISORY_LOCK_ID})"),
    );

    let res: bool = db
        .query_one_raw(statement)
        .await?
        .expect("pg_try_advisory_xact_lock should always return a row")
        .try_get_by_index(0)?;

    Ok(res)
}

fn extract_cache_metadata_from_model(run: &cisa_kev_sync_runs::Model) -> CacheMetadata {
    CacheMetadata {
        etag: run.etag.clone(),
        last_modified: run.last_modified.clone(),
    }
}

fn header_to_string(header_value: Option<&HeaderValue>) -> Option<String> {
    header_value
        // If the header is not entirely visible ASCII characters, return None
        .and_then(|h| h.to_str().ok())
        .map(ToOwned::to_owned)
}

fn extract_cache_metadata_from_headers(resp: &Response) -> CacheMetadata {
    let headers = resp.headers();
    CacheMetadata {
        etag: header_to_string(headers.get(ETAG)),
        last_modified: header_to_string(headers.get(LAST_MODIFIED)),
    }
}

fn cache_metadata_for_response(
    status: StatusCode,
    previous: Option<&CacheMetadata>,
    response: CacheMetadata,
) -> CacheMetadata {
    if status != StatusCode::NOT_MODIFIED {
        return response;
    }

    CacheMetadata {
        etag: response.etag.or_else(|| {
            let metadata = previous?;
            metadata.etag.clone()
        }),
        last_modified: response.last_modified.or_else(|| {
            let metadata = previous?;
            metadata.last_modified.clone()
        }),
    }
}

fn generate_cache_headers(cache_metadata: &CacheMetadata) -> HeaderMap {
    let mut headers = HeaderMap::new();

    // The appropriate header to send with the ETag value is If-None-Match
    if let Some(etag) = &cache_metadata.etag {
        // If the string cannot be converted to a HeaderValue, ignore it
        if let Ok(value) = HeaderValue::from_str(etag) {
            headers.insert(IF_NONE_MATCH, value);
        }
    }

    // The appropriate header to send with the Last-Modified value is If-Modified-Since
    if let Some(last_modified) = &cache_metadata.last_modified {
        // If the string cannot be converted to a HeaderValue, ignore it
        if let Ok(value) = HeaderValue::from_str(last_modified) {
            headers.insert(IF_MODIFIED_SINCE, value);
        }
    }

    headers
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use camino::Utf8PathBuf;
    use sea_orm::{DbBackend, MockDatabase, MockExecResult, Value};
    use secrecy::ExposeSecret as _;
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::time::Duration;
    use url::Url;

    const CONFIG_PATH_ENV: &str = "NV_POSTGRES_INTEGRATION_CONFIG_PATH";
    const DEFAULT_CONFIG_PATH: &str = "src/cve/testdata/nv-server.integration.spookey";

    #[test]
    #[ignore = "requires a disposable Postgres test database"]
    fn failed_sync_run_commits_failure_details() {
        run_async(async {
            let db = connect_to_integration_database().await;
            cisa_kev_sync_runs::Entity::delete_many()
                .exec(&db)
                .await
                .expect("KEV sync runs should clear");

            let transaction = db.begin().await.expect("transaction should begin");
            let sync_run = create_sync_run(&transaction)
                .await
                .expect("sync run should be created");
            update_sync_run_result_failure(
                sync_run,
                "upstream request timed out".to_owned(),
                &transaction,
                slog::Logger::root(slog::Discard, slog::o!()),
            )
            .await
            .expect("failure details should be stored");
            transaction
                .commit()
                .await
                .expect("transaction should commit");

            let run = cisa_kev_sync_runs::Entity::find()
                .one(&db)
                .await
                .expect("sync-run lookup should succeed")
                .expect("sync run should be stored");
            assert_eq!(run.status, "failed");
            assert_eq!(run.error.as_deref(), Some("upstream request timed out"));

            cisa_kev_sync_runs::Entity::delete_many()
                .exec(&db)
                .await
                .expect("KEV sync runs should clear");
        });
    }

    #[test]
    fn store_all_entries_upserts_payload_with_change_aware_timestamps() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([Vec::<BTreeMap<String, Value>>::new()])
            .append_exec_results([MockExecResult {
                last_insert_id: 0,
                rows_affected: 1,
            }])
            .into_connection();

        run_async(store_all_entries(
            &[kev_entry("initial description")],
            &db,
            slog::Logger::root(slog::Discard, slog::o!()),
        ))
        .expect("entry write should succeed");

        let transaction_log = db.into_transaction_log();
        let upsert_sql = transaction_log
            .iter()
            .map(|entry| &entry.statements()[0].sql)
            .find(|sql| sql.contains(r#"INSERT INTO "public"."cisa_kev_entries""#))
            .expect("entry write should issue an insert");
        assert!(upsert_sql.contains("ON CONFLICT"));
        assert!(upsert_sql.contains(r#""entry" = "excluded"."entry""#));
        assert!(upsert_sql.contains(r#""last_seen_at" = CURRENT_TIMESTAMP"#));
        assert!(upsert_sql.contains("IS DISTINCT FROM EXCLUDED.entry"));
        assert!(upsert_sql.contains("ELSE cisa_kev_entries.updated_at END"));
        assert!(!upsert_sql.contains(r#""first_seen_at" = "#));
    }

    #[test]
    fn store_all_entries_rejects_invalid_catalog_atomically() {
        let db = MockDatabase::new(DbBackend::Postgres).into_connection();
        let invalid_entry = json!({
            "cveID": "CVE-2026-1001"
        });

        let error = run_async(store_all_entries(
            &[kev_entry("valid entry"), invalid_entry],
            &db,
            slog::Logger::root(slog::Discard, slog::o!()),
        ))
        .expect_err("invalid catalog entry should fail the entire write");

        assert!(matches!(error, KevError::InvalidCatalogEntry(_)));
        assert!(
            db.into_transaction_log().is_empty(),
            "an invalid entry must prevent every catalog upsert"
        );
    }

    #[test]
    fn kev_refresh_interval_span_uses_milliseconds() {
        assert_eq!(
            kev_refresh_interval_span(DEFAULT_KEV_REFRESH_INTERVAL_MILLISECONDS).get_milliseconds(),
            3_600_000
        );
        assert_eq!(
            kev_refresh_interval_span(900_000).get_milliseconds(),
            900_000
        );
    }

    #[test]
    fn not_modified_response_preserves_absent_cache_validators() {
        let metadata = cache_metadata_for_response(
            StatusCode::NOT_MODIFIED,
            Some(&cache_metadata(Some("old-etag"), Some("old-modified"))),
            cache_metadata(None, None),
        );

        assert_eq!(
            metadata,
            cache_metadata(Some("old-etag"), Some("old-modified"))
        );
    }

    #[test]
    fn not_modified_response_replaces_supplied_cache_validators() {
        let metadata = cache_metadata_for_response(
            StatusCode::NOT_MODIFIED,
            Some(&cache_metadata(Some("old-etag"), Some("old-modified"))),
            cache_metadata(Some("new-etag"), None),
        );

        assert_eq!(
            metadata,
            cache_metadata(Some("new-etag"), Some("old-modified"))
        );
    }

    #[test]
    fn not_modified_response_without_prior_metadata_stays_empty() {
        let metadata =
            cache_metadata_for_response(StatusCode::NOT_MODIFIED, None, cache_metadata(None, None));

        assert_eq!(metadata, cache_metadata(None, None));
    }

    #[test]
    fn full_response_does_not_preserve_stale_cache_validators() {
        let metadata = cache_metadata_for_response(
            StatusCode::OK,
            Some(&cache_metadata(Some("old-etag"), Some("old-modified"))),
            cache_metadata(None, None),
        );

        assert_eq!(metadata, cache_metadata(None, None));
    }

    fn cache_metadata(etag: Option<&str>, last_modified: Option<&str>) -> CacheMetadata {
        CacheMetadata {
            etag: etag.map(str::to_owned),
            last_modified: last_modified.map(str::to_owned),
        }
    }

    #[test]
    #[ignore = "requires a disposable Postgres test database"]
    fn repeated_entries_refresh_payload_and_timestamps() {
        run_async(async {
            let db = connect_to_integration_database().await;
            cisa_kev_entries::Entity::delete_many()
                .exec(&db)
                .await
                .expect("KEV entries should clear");

            let initial_entry = kev_entry("initial description");
            write_entries(&db, std::slice::from_ref(&initial_entry)).await;
            let initial = read_entry(&db).await;

            tokio::time::sleep(Duration::from_millis(5)).await;
            write_entries(&db, &[initial_entry]).await;
            let unchanged = read_entry(&db).await;
            assert_eq!(unchanged.entry, initial.entry);
            assert_eq!(unchanged.first_seen_at, initial.first_seen_at);
            assert!(unchanged.last_seen_at > initial.last_seen_at);
            assert_eq!(unchanged.updated_at, initial.updated_at);

            tokio::time::sleep(Duration::from_millis(5)).await;
            let changed_entry = kev_entry("changed description");
            write_entries(&db, std::slice::from_ref(&changed_entry)).await;
            let changed = read_entry(&db).await;
            assert_eq!(changed.entry, changed_entry);
            assert_eq!(changed.first_seen_at, initial.first_seen_at);
            assert!(changed.last_seen_at > unchanged.last_seen_at);
            assert!(changed.updated_at > unchanged.updated_at);

            cisa_kev_entries::Entity::delete_many()
                .exec(&db)
                .await
                .expect("KEV entries should clear");
        });
    }

    async fn write_entries(db: &DatabaseConnection, entries: &[serde_json::Value]) {
        let transaction = db.begin().await.expect("transaction should begin");
        store_all_entries(
            entries,
            &transaction,
            slog::Logger::root(slog::Discard, slog::o!()),
        )
        .await
        .expect("entry write should succeed");
        transaction
            .commit()
            .await
            .expect("transaction should commit");
    }

    async fn read_entry(db: &DatabaseConnection) -> cisa_kev_entries::Model {
        cisa_kev_entries::Entity::find_by_id("CVE-2026-1000".to_owned())
            .one(db)
            .await
            .expect("entry lookup should succeed")
            .expect("entry should be stored")
    }

    fn kev_entry(short_description: &str) -> serde_json::Value {
        json!({
            "cveID": "CVE-2026-1000",
            "cwes": [],
            "dateAdded": "2026-01-01",
            "dueDate": "2026-02-01",
            "product": "example product",
            "requiredAction": "apply the update",
            "shortDescription": short_description,
            "vendorProject": "example vendor",
            "vulnerabilityName": "Example vulnerability"
        })
    }

    async fn connect_to_integration_database() -> DatabaseConnection {
        let config_path = integration_config_path();
        let config = Config::parse(&config_path).expect("integration-test config should parse");
        let database_url = config.database_connection().expose_secret();
        assert_disposable_database_url(database_url);

        crate::db::connection(&config).await.unwrap_or_else(|error| {
            panic!(
                "Postgres integration database should connect and migrate. Config path: {config_path}. Database: {}. Original error: {error:#}",
                database_name(database_url),
            )
        })
    }

    fn integration_config_path() -> Utf8PathBuf {
        std::env::var(CONFIG_PATH_ENV).map_or_else(
            |_| {
                let mut path = Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"));
                path.push(DEFAULT_CONFIG_PATH);
                path
            },
            Utf8PathBuf::from,
        )
    }

    fn assert_disposable_database_url(database_url: &str) {
        let url = Url::parse(database_url).expect("database URL should parse");
        assert!(
            matches!(url.scheme(), "postgres" | "postgresql"),
            "integration test database connection must use a Postgres URL"
        );
        let database = database_name(database_url);
        assert!(
            database.contains("test") || database.contains("integration"),
            "integration test database name must contain 'test' or 'integration'; got {database:?}"
        );
    }

    fn database_name(database_url: &str) -> String {
        Url::parse(database_url)
            .ok()
            .map(|url| url.path().trim_start_matches('/').to_owned())
            .filter(|database| !database.is_empty())
            .unwrap_or_else(|| "<unknown>".to_owned())
    }

    fn run_async<T>(future: impl std::future::Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime should build")
            .block_on(future)
    }
}
