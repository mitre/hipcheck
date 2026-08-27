use crate::config::Config;
use crate::db::DatabaseConnectionError;
use crate::db::entities::cisa_kev_entries;
use crate::db::entities::cisa_kev_sync_runs;
use crate::error::{ErrorSourceIterator as _, format_error_chain};
use jiff::civil::Date;
use reqwest::header::{
    ETAG, HeaderMap, HeaderValue, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED,
};
use reqwest::{Client, Response, StatusCode, Url};
use sea_orm::{
    ActiveModelTrait as _, ColumnTrait as _, ConnectionTrait, EntityTrait as _,
    PaginatorTrait as _, QueryFilter as _, QueryOrder as _, TransactionTrait as _, Value,
};
use sea_orm::{
    DatabaseBackend, DatabaseConnection, DatabaseTransaction, Set, Statement, sea_query::Expr,
};
use serde::Deserialize;
use sha2::{Digest as _, Sha256};
use slog::{Logger, debug, error, info, warn};
use std::{
    borrow::ToOwned,
    collections::HashMap,
    error::Error as _,
    fmt::{Debug, Display, Write as _},
    time::{Duration, Instant},
};
use tokio::time::sleep;

pub const DEFAULT_KEV_URL: &str =
    "https://www.cisa.gov/sites/default/files/feeds/known_exploited_vulnerabilities.json";
pub const DEFAULT_KEV_REFRESH_INTERVAL_MILLISECONDS: u64 = 3_600_000;
pub const DEFAULT_KEV_RESPONSE_BODY_MAX_BYTES: usize = 16 * 1024 * 1024;

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
    CatalogCountMismatch { declared: i64, actual: usize },
    CatalogCountOutOfRange(i64),
    CatalogResponseBodyTooLarge { max_bytes: usize },
    ConflictingDuplicateCveId(String),
    DatabaseConnectionError(DatabaseConnectionError),
    InvalidCatalog(serde_json::Error),
    InvalidCatalogEntry(serde_json::Error),
    InvalidCatalogReleaseDate(chrono::ParseError),
    EntryCountOutOfRange(usize),
    ReqwestError(reqwest::Error),
    SeaOrmError(sea_orm::DbErr),
    SyncRunNotFound(i64),
    TransactionLockError,
    UnexpectedResponseStatus(StatusCode),
}

#[derive(Debug, PartialEq, Eq)]
pub struct FailRunningKevSyncRunsSummary {
    pub sync_runs_failed: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ResetKevStorageSummary {
    pub entries_deleted: u64,
    pub sync_runs_deleted: u64,
}

#[derive(Debug)]
pub enum ResetKevStorageError {
    Db(sea_orm::DbErr),
    RunningSyncRuns(u64),
    SyncAlreadyRunning,
    SyncLock(KevError),
}

impl Display for ResetKevStorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Db(_) => write!(f, "failed to reset KEV storage"),
            Self::RunningSyncRuns(count) => {
                write!(
                    f,
                    "refusing to reset KEV storage while {count} sync run(s) are running"
                )
            }
            Self::SyncAlreadyRunning => write!(f, "a KEV sync run is already running"),
            Self::SyncLock(_) => write!(f, "failed to check KEV sync advisory lock"),
        }
    }
}

impl std::error::Error for ResetKevStorageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Db(error) => Some(error),
            Self::SyncLock(error) => Some(error),
            Self::RunningSyncRuns(_) | Self::SyncAlreadyRunning => None,
        }
    }
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
            Self::CatalogCountMismatch { declared, actual } => {
                write!(
                    f,
                    "KEV catalog count {declared} does not match {actual} vulnerability entries"
                )
            }
            Self::CatalogCountOutOfRange(count) => {
                write!(f, "KEV catalog count {count} exceeds the database range")
            }
            Self::EntryCountOutOfRange(count) => {
                write!(f, "KEV entry count {count} exceeds the database range")
            }
            Self::CatalogResponseBodyTooLarge { max_bytes } => {
                write!(
                    f,
                    "KEV catalog response body exceeds configured limit of {max_bytes} bytes"
                )
            }
            Self::ConflictingDuplicateCveId(cve_id) => {
                write!(
                    f,
                    "KEV catalog contains conflicting entries for CVE ID {cve_id}"
                )
            }
            Self::DatabaseConnectionError(_) => {
                write!(f, "failed to connect to database")
            }
            Self::InvalidCatalog(_) => {
                write!(f, "failed to parse KEV catalog")
            }
            Self::InvalidCatalogEntry(_) => {
                write!(f, "failed to parse KEV catalog entry")
            }
            Self::InvalidCatalogReleaseDate(_) => {
                write!(f, "failed to parse KEV catalog release date")
            }
            Self::ReqwestError(_) => {
                write!(f, "failed to fetch KEV data")
            }
            Self::SeaOrmError(_) => {
                write!(f, "failed to access database")
            }
            Self::SyncRunNotFound(generation) => {
                write!(f, "KEV sync run {generation} was not found")
            }
            Self::TransactionLockError => {
                write!(f, "failed to acquire exclusive database lock to sync data")
            }
            Self::UnexpectedResponseStatus(status) => {
                write!(
                    f,
                    "KEV catalog request returned unexpected HTTP status {status}"
                )
            }
        }
    }
}

impl std::error::Error for KevError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::CatalogCountMismatch { .. } => None,
            Self::CatalogCountOutOfRange(_) => None,
            Self::EntryCountOutOfRange(_) => None,
            Self::CatalogResponseBodyTooLarge { .. } => None,
            Self::ConflictingDuplicateCveId(_) => None,
            Self::DatabaseConnectionError(err) => Some(err),
            Self::InvalidCatalog(err) => Some(err),
            Self::InvalidCatalogEntry(err) => Some(err),
            Self::InvalidCatalogReleaseDate(err) => Some(err),
            Self::ReqwestError(err) => Some(err),
            Self::SeaOrmError(err) => Some(err),
            Self::SyncRunNotFound(_) => None,
            Self::TransactionLockError => None,
            Self::UnexpectedResponseStatus(_) => None,
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
    kev_response_body_max_bytes: usize,
    kev_url: Option<reqwest::Url>,
}

impl KevConfig {
    fn from_config(config: &Config) -> Self {
        let refresh_interval_milliseconds = config
            .kev_refresh_interval
            .unwrap_or(DEFAULT_KEV_REFRESH_INTERVAL_MILLISECONDS);
        Self {
            kev_refresh_interval: kev_refresh_interval_span(refresh_interval_milliseconds),
            kev_response_body_max_bytes: config
                .kev_response_body_max_bytes
                .unwrap_or(DEFAULT_KEV_RESPONSE_BODY_MAX_BYTES),
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum KevSyncRunStatus {
    Success,
    Failed,
    NotModified,
}

impl Display for KevSyncRunStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let status = match self {
            Self::Success => "success",
            Self::Failed => "failed",
            Self::NotModified => "not_modified",
        };
        f.write_str(status)
    }
}

#[derive(Clone, Debug)]
struct FinishKevSyncRun {
    status: KevSyncRunStatus,
    catalog_version: Option<String>,
    catalog_date_released: Option<sea_orm::prelude::DateTimeWithTimeZone>,
    catalog_count: Option<i32>,
    content_sha256: Option<String>,
    records_seen: i32,
    records_inserted: i32,
    records_updated: i32,
    error: Option<String>,
}

impl FinishKevSyncRun {
    fn failed(error: String) -> Self {
        Self {
            status: KevSyncRunStatus::Failed,
            catalog_version: None,
            catalog_date_released: None,
            catalog_count: None,
            content_sha256: None,
            records_seen: 0,
            records_inserted: 0,
            records_updated: 0,
            error: Some(error),
        }
    }

    fn not_modified() -> Self {
        Self {
            status: KevSyncRunStatus::NotModified,
            catalog_version: None,
            catalog_date_released: None,
            catalog_count: None,
            content_sha256: None,
            records_seen: 0,
            records_inserted: 0,
            records_updated: 0,
            error: None,
        }
    }
}

#[derive(Debug)]
struct RunSummary {
    records_seen: i32,
    records_inserted: i32,
    records_updated: i32,
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
    let res = fetch_kev(request_mode, config, &sync_run, &xact, log.clone()).await;
    let completion = match &res {
        Ok(completion) => completion.clone(),
        Err(error) => FinishKevSyncRun::failed(format_error_chain(error)),
    };
    finish_kev_sync_run(&xact, sync_run.generation, completion).await?;
    // Must finish the transaction first, regardless of whether there was an error
    // during processing.
    finish_locked_kev_sync(xact).await?;
    res.map(|_| ())
}

/// TODO: name this function better
/// Cancellation: TODO
async fn fetch_kev(
    request_mode: RequestMode,
    config: &KevConfig,
    sync_run: &cisa_kev_sync_runs::Model,
    xact: &DatabaseTransaction,
    log: Logger,
) -> Result<FinishKevSyncRun, KevError> {
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

    // TODO
    // save expires and cache-control from response
    // cache-control max-age directive provides number of seconds until server
    // considers it stale; use that as time to refresh
    let request_diagnostics = KevRequestDiagnostics::new(
        sync_run.generation,
        matches!(request_mode, RequestMode::UseConditionalRequest),
        &url,
    );
    let response = request_kev_catalog(&client, url, headers, &request_diagnostics, &log).await?;

    let status = response.status();
    let cache_metadata = cache_metadata_for_response(
        status,
        previous_cache_metadata.as_ref(),
        extract_cache_metadata_from_headers(&response),
    );
    debug!(log, "KEV Sync: cache metadata: {cache_metadata:?}");

    // Detect 304 Not Modified and exit early
    if status == StatusCode::NOT_MODIFIED {
        update_sync_run_cache_metadata(cache_metadata, sync_run.generation, xact).await?;
        info!(
            log,
            "KEV Catalog Response was 304 Not Modified; nothing to process"
        );
        return Ok(FinishKevSyncRun::not_modified());
    }

    let body = read_kev_catalog_body(response, config.kev_response_body_max_bytes).await?;
    let content_sha256 = sha256_hex(&body);
    let catalog = serde_json::from_slice::<KevCatalog>(&body).map_err(KevError::InvalidCatalog)?;
    let catalog_date_released: sea_orm::prelude::DateTimeWithTimeZone = catalog
        .date_released
        .parse()
        .map_err(KevError::InvalidCatalogReleaseDate)?;
    validate_catalog_count(catalog.count, catalog.vulnerabilities.len())?;
    let catalog_count = i32::try_from(catalog.count)
        .map_err(|_| KevError::CatalogCountOutOfRange(catalog.count))?;

    let catalog_version = &catalog.catalog_version;
    let count = catalog.count;
    let date_released = &catalog.date_released;
    debug!(log, "KEV Catalog received";
    "catalog_version" => catalog_version,
    "count" => count,
    "date_released" => date_released
    );

    let summary = store_all_entries(&catalog.vulnerabilities, xact, log.clone()).await?;
    retire_missing_kev_entries(xact, &catalog.vulnerabilities, catalog_date_released).await?;

    // Only save response validators after every catalog entry has been stored.
    // Otherwise a later conditional request could hide an entry that failed validation.
    update_sync_run_cache_metadata(cache_metadata, sync_run.generation, xact).await?;

    Ok(FinishKevSyncRun {
        status: KevSyncRunStatus::Success,
        catalog_version: Some(catalog.catalog_version),
        catalog_date_released: Some(catalog_date_released),
        catalog_count: Some(catalog_count),
        content_sha256: Some(content_sha256),
        records_seen: summary.records_seen,
        records_inserted: summary.records_inserted,
        records_updated: summary.records_updated,
        error: None,
    })
}

async fn read_kev_catalog_body(response: Response, max_bytes: usize) -> Result<Vec<u8>, KevError> {
    let content_length = response.content_length();
    validate_kev_catalog_content_length(content_length, max_bytes)?;

    let mut body = Vec::with_capacity(kev_catalog_body_capacity(content_length, max_bytes));
    let mut response = response;
    while let Some(chunk) = response.chunk().await? {
        append_kev_catalog_chunk(&mut body, &chunk, max_bytes)?;
    }

    Ok(body)
}

fn validate_kev_catalog_content_length(
    content_length: Option<u64>,
    max_bytes: usize,
) -> Result<(), KevError> {
    if content_length.is_some_and(|length| length > max_bytes as u64) {
        return Err(KevError::CatalogResponseBodyTooLarge { max_bytes });
    }

    Ok(())
}

fn kev_catalog_body_capacity(content_length: Option<u64>, max_bytes: usize) -> usize {
    content_length
        .and_then(|length| usize::try_from(length).ok())
        .unwrap_or_default()
        .min(max_bytes)
}

fn append_kev_catalog_chunk(
    body: &mut Vec<u8>,
    chunk: &[u8],
    max_bytes: usize,
) -> Result<(), KevError> {
    if chunk.len() > max_bytes.saturating_sub(body.len()) {
        return Err(KevError::CatalogResponseBodyTooLarge { max_bytes });
    }

    body.extend_from_slice(chunk);
    Ok(())
}

async fn request_kev_catalog(
    client: &Client,
    url: Url,
    headers: HeaderMap,
    diagnostics: &KevRequestDiagnostics,
    log: &Logger,
) -> Result<Response, KevError> {
    let request = client.get(url.clone()).headers(headers);
    debug!(log, "KEV Sync: sending KEV catalog request";
        "generation" => diagnostics.generation,
        "conditional_request" => diagnostics.conditional_request,
        "endpoint_scheme" => diagnostics.endpoint.scheme.as_str(),
        "endpoint_host" => diagnostics.endpoint.host.as_str(),
        "endpoint_port" => diagnostics.endpoint.port,
        "http_proxy_configured" => diagnostics.proxy_environment.http_proxy_configured,
        "https_proxy_configured" => diagnostics.proxy_environment.https_proxy_configured,
        "no_proxy_configured" => diagnostics.proxy_environment.no_proxy_configured);

    let started = Instant::now();
    let response = match request.send().await {
        Ok(response) => response,
        Err(error) => {
            let failure_class = KevRequestFailureClass::from_reqwest_error(&error);
            warn!(log, "KEV Sync: KEV catalog request failed";
                "generation" => diagnostics.generation,
                "conditional_request" => diagnostics.conditional_request,
                "endpoint_scheme" => diagnostics.endpoint.scheme.as_str(),
                "endpoint_host" => diagnostics.endpoint.host.as_str(),
                "endpoint_port" => diagnostics.endpoint.port,
                "elapsed_milliseconds" => elapsed_milliseconds(started.elapsed()),
                "failure_class" => failure_class.as_str(),
                "is_timeout" => error.is_timeout(),
                "is_connect" => error.is_connect(),
                "is_request" => error.is_request(),
                "is_body" => error.is_body(),
                "is_decode" => error.is_decode(),
                "http_proxy_configured" => diagnostics.proxy_environment.http_proxy_configured,
                "https_proxy_configured" => diagnostics.proxy_environment.https_proxy_configured,
                "no_proxy_configured" => diagnostics.proxy_environment.no_proxy_configured,
                "error_sources" => ?reqwest_error_source_messages(&error));
            return Err(KevError::ReqwestError(error));
        }
    };
    let final_endpoint = KevEndpoint::from_url(response.url());
    debug!(log, "KEV Sync: received KEV catalog response";
        "generation" => diagnostics.generation,
        "conditional_request" => diagnostics.conditional_request,
        "final_endpoint_scheme" => final_endpoint.scheme.as_str(),
        "final_endpoint_host" => final_endpoint.host.as_str(),
        "final_endpoint_port" => final_endpoint.port,
        "http_version" => format!("{:?}", response.version()),
        "status" => response.status().as_u16(),
        "elapsed_milliseconds" => elapsed_milliseconds(started.elapsed()));

    let status = response.status();
    if status != StatusCode::OK && status != StatusCode::NOT_MODIFIED {
        return Err(KevError::UnexpectedResponseStatus(status));
    }

    Ok(response)
}

/// Redacted connection details for one KEV catalog request.
#[derive(Debug, Clone, PartialEq, Eq)]
struct KevRequestDiagnostics {
    generation: i64,
    conditional_request: bool,
    endpoint: KevEndpoint,
    proxy_environment: ProxyEnvironment,
}

impl KevRequestDiagnostics {
    fn new(generation: i64, conditional_request: bool, url: &Url) -> Self {
        Self {
            generation,
            conditional_request,
            endpoint: KevEndpoint::from_url(url),
            proxy_environment: ProxyEnvironment::from_environment(),
        }
    }
}

/// The URL components that are safe to include in KEV request logs.
#[derive(Debug, Clone, PartialEq, Eq)]
struct KevEndpoint {
    scheme: String,
    host: String,
    port: Option<u16>,
}

impl KevEndpoint {
    fn from_url(url: &Url) -> Self {
        Self {
            scheme: url.scheme().to_owned(),
            host: url.host_str().unwrap_or("<none>").to_owned(),
            port: url.port_or_known_default(),
        }
    }
}

/// Whether conventional proxy environment variables are set, without exposing their values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ProxyEnvironment {
    http_proxy_configured: bool,
    https_proxy_configured: bool,
    no_proxy_configured: bool,
}

impl ProxyEnvironment {
    fn from_environment() -> Self {
        Self::from_lookup(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()))
    }

    fn from_lookup(lookup: impl Fn(&str) -> bool) -> Self {
        Self {
            http_proxy_configured: lookup("HTTP_PROXY") || lookup("http_proxy"),
            https_proxy_configured: lookup("HTTPS_PROXY") || lookup("https_proxy"),
            no_proxy_configured: lookup("NO_PROXY") || lookup("no_proxy"),
        }
    }
}

/// A stable, high-level classification for a failed KEV HTTP request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KevRequestFailureClass {
    Timeout,
    Connect,
    Request,
    Body,
    Decode,
    Other,
}

impl KevRequestFailureClass {
    fn from_reqwest_error(error: &reqwest::Error) -> Self {
        Self::from_flags(ReqwestErrorFlags {
            timeout: error.is_timeout(),
            connect: error.is_connect(),
            request: error.is_request(),
            body: error.is_body(),
            decode: error.is_decode(),
        })
    }

    fn from_flags(flags: ReqwestErrorFlags) -> Self {
        if flags.timeout {
            Self::Timeout
        } else if flags.connect {
            Self::Connect
        } else if flags.request {
            Self::Request
        } else if flags.body {
            Self::Body
        } else if flags.decode {
            Self::Decode
        } else {
            Self::Other
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Timeout => "timeout",
            Self::Connect => "connect",
            Self::Request => "request",
            Self::Body => "body",
            Self::Decode => "decode",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct ReqwestErrorFlags {
    timeout: bool,
    connect: bool,
    request: bool,
    body: bool,
    decode: bool,
}

fn elapsed_milliseconds(elapsed: Duration) -> u64 {
    u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
}

/// Return nested error messages without the top-level Reqwest error, which can include a URL.
fn reqwest_error_source_messages(error: &reqwest::Error) -> Vec<String> {
    error
        .sources_iter()
        .skip(1)
        .map(std::string::ToString::to_string)
        .collect()
}

fn validate_catalog_count(declared: i64, actual: usize) -> Result<(), KevError> {
    if declared < 0 || usize::try_from(declared).ok() != Some(actual) {
        return Err(KevError::CatalogCountMismatch { declared, actual });
    }

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

fn convert_one_entry(value: &serde_json::Value) -> Result<KevEntry, serde_json::Error> {
    let vulnerability: Vulnerability = serde_json::from_value(value.clone())?;

    Ok(KevEntry {
        cve_id: vulnerability.cve_id,
        entry: value.clone(),
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
    let entries = values
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
    let entries = deduplicate_entries(entries, &log)?;
    let records_seen = count_to_i32(entries.len())?;
    let outcomes = if entries.is_empty() {
        Vec::new()
    } else {
        db.query_all_raw(kev_entry_upsert_statement(&entries))
            .await?
    };
    let records_inserted = count_to_i32(
        outcomes
            .iter()
            .map(|outcome| outcome.try_get_by_index::<bool>(0))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|inserted| *inserted)
            .count(),
    )?;
    let records_updated = count_to_i32(
        outcomes
            .iter()
            .map(|outcome| outcome.try_get_by_index::<bool>(1))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|updated| *updated)
            .count(),
    )?;
    debug!(log, "KEV Sync: stored catalog entries";
        "records_seen" => records_seen,
        "records_inserted" => records_inserted,
        "records_updated" => records_updated);

    let summary = RunSummary {
        records_seen,
        records_inserted,
        records_updated,
    };
    Ok(summary)
}

struct KevEntry {
    cve_id: String,
    entry: serde_json::Value,
}

/// Build the PostgreSQL 18 upsert used for KEV entries.
///
/// The old/new aliases in `RETURNING` report each row's actual write outcome
/// without a separate pre-write lookup.
fn kev_entry_upsert_statement(entries: &[KevEntry]) -> Statement {
    assert!(
        !entries.is_empty(),
        "KEV entry upsert requires at least one entry"
    );

    let mut sql =
        String::from("INSERT INTO \"public\".\"cisa_kev_entries\" (\"cve_id\", \"entry\") VALUES ");
    let mut values = Vec::new();

    for (index, entry) in entries.iter().enumerate() {
        if index > 0 {
            sql.push_str(", ");
        }
        let cve_id_parameter = next_kev_upsert_parameter(&values);
        values.push(Value::String(Some(entry.cve_id.clone())));
        let entry_parameter = next_kev_upsert_parameter(&values);
        values.push(Value::Json(Some(Box::new(entry.entry.clone()))));
        write!(sql, "(${cve_id_parameter}, ${entry_parameter})")
            .expect("writing KEV upsert SQL should not fail");
    }

    sql.push_str(
        " ON CONFLICT (\"cve_id\") DO UPDATE SET \
         \"entry\" = EXCLUDED.\"entry\", \
         \"last_seen_at\" = CURRENT_TIMESTAMP, \
         \"removed_at\" = NULL, \
         \"updated_at\" = CASE WHEN \"cisa_kev_entries\".\"entry\" \
         IS DISTINCT FROM EXCLUDED.\"entry\" THEN CURRENT_TIMESTAMP \
         ELSE \"cisa_kev_entries\".\"updated_at\" END \
         RETURNING WITH (OLD AS old, NEW AS new) \
         old.\"cve_id\" IS NULL AS inserted, \
         old.\"entry\" IS DISTINCT FROM new.\"entry\" AS updated",
    );

    Statement::from_sql_and_values(DatabaseBackend::Postgres, sql, values)
}

fn next_kev_upsert_parameter(values: &[Value]) -> usize {
    values
        .len()
        .checked_add(1)
        .expect("KEV upsert parameter count should fit in usize")
}

async fn retire_missing_kev_entries<C>(
    db: &C,
    values: &[serde_json::Value],
    removed_at: sea_orm::prelude::DateTimeWithTimeZone,
) -> Result<(), KevError>
where
    C: ConnectionTrait,
{
    let cve_ids = values
        .iter()
        .map(convert_one_entry)
        .collect::<Result<Vec<_>, _>>()
        .map_err(KevError::InvalidCatalogEntry)?
        .into_iter()
        .map(|entry| entry.cve_id)
        .collect::<Vec<_>>();
    let mut update = cisa_kev_entries::Entity::update_many()
        .col_expr(cisa_kev_entries::Column::RemovedAt, Expr::value(removed_at))
        .filter(cisa_kev_entries::Column::RemovedAt.is_null());
    if !cve_ids.is_empty() {
        update = update.filter(cisa_kev_entries::Column::CveId.is_not_in(cve_ids));
    }
    update.exec(db).await?;
    Ok(())
}

fn deduplicate_entries(entries: Vec<KevEntry>, log: &Logger) -> Result<Vec<KevEntry>, KevError> {
    let mut entry_indexes: HashMap<String, usize> = HashMap::new();
    let mut unique_entries: Vec<KevEntry> = Vec::with_capacity(entries.len());

    for entry in entries {
        if let Some(&index) = entry_indexes.get(&entry.cve_id) {
            let retained = &unique_entries[index];
            if retained.entry != entry.entry {
                return Err(KevError::ConflictingDuplicateCveId(entry.cve_id));
            }

            warn!(log, "KEV Sync: skipping duplicate KEV catalog entry";
                "cve_id" => entry.cve_id);
            continue;
        }

        entry_indexes.insert(entry.cve_id.clone(), unique_entries.len());
        unique_entries.push(entry);
    }

    Ok(unique_entries)
}

fn count_to_i32(value: usize) -> Result<i32, KevError> {
    i32::try_from(value).map_err(|_| KevError::EntryCountOutOfRange(value))
}

/// Cancellation: TODO
async fn get_latest_run(
    xact: &DatabaseTransaction,
) -> Result<Option<cisa_kev_sync_runs::Model>, KevError> {
    let run: Option<cisa_kev_sync_runs::Model> = cisa_kev_sync_runs::Entity::find()
        .filter(cisa_kev_sync_runs::Column::Status.is_in(["success", "not_modified"]))
        .order_by_desc(cisa_kev_sync_runs::Column::Generation)
        .one(xact)
        .await?;
    Ok(run)
}

/// Cancellation: TODO
async fn create_sync_run(
    xact: &DatabaseTransaction,
) -> Result<cisa_kev_sync_runs::Model, KevError> {
    let run = cisa_kev_sync_runs::ActiveModel {
        status: Set("running".to_owned()),
        ..Default::default()
    };
    run.insert(xact).await.map_err(KevError::SeaOrmError)
}

/// Cancellation: TODO
async fn update_sync_run_cache_metadata(
    cache_metadata: CacheMetadata,
    generation: i64,
    xact: &DatabaseTransaction,
) -> Result<(), KevError> {
    let result = cisa_kev_sync_runs::Entity::update_many()
        .col_expr(
            cisa_kev_sync_runs::Column::Etag,
            Expr::value(cache_metadata.etag),
        )
        .col_expr(
            cisa_kev_sync_runs::Column::LastModified,
            Expr::value(cache_metadata.last_modified),
        )
        .filter(cisa_kev_sync_runs::Column::Generation.eq(generation))
        .exec(xact)
        .await?;
    if result.rows_affected == 0 {
        return Err(KevError::SyncRunNotFound(generation));
    }
    Ok(())
}

async fn finish_kev_sync_run<C>(
    db: &C,
    generation: i64,
    completion: FinishKevSyncRun,
) -> Result<(), KevError>
where
    C: ConnectionTrait,
{
    let result = cisa_kev_sync_runs::Entity::update_many()
        .col_expr(
            cisa_kev_sync_runs::Column::CompletedAt,
            Expr::current_timestamp(),
        )
        .col_expr(
            cisa_kev_sync_runs::Column::Status,
            Expr::value(completion.status.to_string()),
        )
        .col_expr(
            cisa_kev_sync_runs::Column::CatalogVersion,
            Expr::value(completion.catalog_version),
        )
        .col_expr(
            cisa_kev_sync_runs::Column::CatalogDateReleased,
            Expr::value(completion.catalog_date_released),
        )
        .col_expr(
            cisa_kev_sync_runs::Column::CatalogCount,
            Expr::value(completion.catalog_count),
        )
        .col_expr(
            cisa_kev_sync_runs::Column::ContentSha256,
            Expr::value(completion.content_sha256),
        )
        .col_expr(
            cisa_kev_sync_runs::Column::RecordsSeen,
            Expr::value(completion.records_seen),
        )
        .col_expr(
            cisa_kev_sync_runs::Column::RecordsInserted,
            Expr::value(completion.records_inserted),
        )
        .col_expr(
            cisa_kev_sync_runs::Column::RecordsUpdated,
            Expr::value(completion.records_updated),
        )
        .col_expr(
            cisa_kev_sync_runs::Column::Error,
            Expr::value(completion.error),
        )
        .filter(cisa_kev_sync_runs::Column::Generation.eq(generation))
        .exec(db)
        .await?;
    if result.rows_affected == 0 {
        return Err(KevError::SyncRunNotFound(generation));
    }
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

/// Try to acquire the transaction-scoped KEV sync advisory lock.
///
/// This uses `pg_try_advisory_xact_lock`, so it returns immediately when the
/// lock is unavailable.
///
/// Callers that need the lock to guard multiple statements must pass an active
/// transaction. Passing a plain connection only guards the current statement.
pub async fn try_acquire_kev_sync_lock<C>(db: &C) -> Result<bool, KevError>
where
    C: ConnectionTrait,
{
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

/// Mark running KEV sync runs failed.
///
/// Callers should only use this after verifying no live sync holds the
/// transaction-scoped advisory lock.
pub async fn fail_running_kev_sync_runs<C>(
    db: &C,
    error: &str,
) -> Result<FailRunningKevSyncRunsSummary, KevError>
where
    C: ConnectionTrait,
{
    let result = cisa_kev_sync_runs::Entity::update_many()
        .col_expr(
            cisa_kev_sync_runs::Column::CompletedAt,
            Expr::current_timestamp(),
        )
        .col_expr(cisa_kev_sync_runs::Column::Status, Expr::value("failed"))
        .col_expr(cisa_kev_sync_runs::Column::Error, Expr::value(error))
        .filter(cisa_kev_sync_runs::Column::Status.eq("running"))
        .exec(db)
        .await?;

    Ok(FailRunningKevSyncRunsSummary {
        sync_runs_failed: result.rows_affected,
    })
}

/// Clear KEV entries and sync-run metadata from database storage.
///
/// Callers must pass an active transaction so the transaction-scoped advisory
/// lock is held until the reset is committed or rolled back.
pub async fn reset_kev_storage<C>(
    db: &C,
    force: bool,
) -> Result<ResetKevStorageSummary, ResetKevStorageError>
where
    C: ConnectionTrait,
{
    let sync_lock_acquired = try_acquire_kev_sync_lock(db)
        .await
        .map_err(ResetKevStorageError::SyncLock)?;
    if !sync_lock_acquired {
        return Err(ResetKevStorageError::SyncAlreadyRunning);
    }

    let running_sync_runs = cisa_kev_sync_runs::Entity::find()
        .filter(cisa_kev_sync_runs::Column::Status.eq("running"))
        .count(db)
        .await
        .map_err(ResetKevStorageError::Db)?;
    if running_sync_runs > 0 && !force {
        return Err(ResetKevStorageError::RunningSyncRuns(running_sync_runs));
    }

    let entries_deleted = cisa_kev_entries::Entity::find()
        .count(db)
        .await
        .map_err(ResetKevStorageError::Db)?;
    let sync_runs_deleted = cisa_kev_sync_runs::Entity::find()
        .count(db)
        .await
        .map_err(ResetKevStorageError::Db)?;
    let truncate = Statement::from_string(
        db.get_database_backend(),
        "TRUNCATE TABLE public.cisa_kev_entries, public.cisa_kev_sync_runs RESTART IDENTITY"
            .to_owned(),
    );

    db.execute_raw(truncate)
        .await
        .map(|_| ResetKevStorageSummary {
            entries_deleted,
            sync_runs_deleted,
        })
        .map_err(ResetKevStorageError::Db)
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

fn sha256_hex(body: &[u8]) -> String {
    format!("{:x}", Sha256::digest(body))
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
    use httpmock::prelude::*;
    use sea_orm::{DbBackend, MockDatabase, MockExecResult, Value};
    use secrecy::ExposeSecret as _;
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::time::Duration;
    use url::Url;

    const CONFIG_PATH_ENV: &str = "NV_POSTGRES_INTEGRATION_CONFIG_PATH";
    const DEFAULT_CONFIG_PATH: &str = "src/cve/testdata/nv-server.integration.spookey";
    const VALID_KEV_CATALOG: &str = r#"{
        "catalogVersion": "2026.08.20",
        "count": 0,
        "dateReleased": "2026-08-20T00:00:00Z",
        "vulnerabilities": []
    }"#;

    fn mock_advisory_lock_row(acquired: bool) -> BTreeMap<String, Value> {
        BTreeMap::from([("pg_try_advisory_xact_lock".to_owned(), acquired.into())])
    }

    fn mock_num_items_row(num_items: i64) -> BTreeMap<String, Value> {
        BTreeMap::from([("num_items".to_owned(), num_items.into())])
    }

    #[test]
    fn kev_catalog_request_accepts_ok_response() {
        let server = MockServer::start();
        let _mock = server.mock(|when, then| {
            when.method(GET).path("/kev");
            then.status(200)
                .header("content-type", "application/json")
                .body(VALID_KEV_CATALOG);
        });
        let log = slog::Logger::root(slog::Discard, slog::o!());
        let url = Url::parse(&server.url("/kev")).expect("mock URL should be valid");
        let diagnostics = test_request_diagnostics(&url);

        let response = run_async(request_kev_catalog(
            &Client::new(),
            url,
            HeaderMap::new(),
            &diagnostics,
            &log,
        ))
        .expect("200 response should be accepted");

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn kev_catalog_request_follows_redirect_and_reports_final_endpoint() {
        let server = MockServer::start();
        let destination = server.url("/kev");
        let _redirect = server.mock(|when, then| {
            when.method(GET).path("/redirect");
            then.status(302).header("location", destination.as_str());
        });
        let _destination = server.mock(|when, then| {
            when.method(GET).path("/kev");
            then.status(200)
                .header("content-type", "application/json")
                .body(VALID_KEV_CATALOG);
        });
        let log = slog::Logger::root(slog::Discard, slog::o!());
        let url = Url::parse(&server.url("/redirect")).expect("mock URL should be valid");
        let diagnostics = test_request_diagnostics(&url);

        let response = run_async(request_kev_catalog(
            &Client::new(),
            url,
            HeaderMap::new(),
            &diagnostics,
            &log,
        ))
        .expect("redirected response should be accepted");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(KevEndpoint::from_url(response.url()).host, "127.0.0.1");
        assert_eq!(
            KevEndpoint::from_url(response.url()).port,
            Some(server.port())
        );
    }

    #[test]
    fn kev_endpoint_excludes_sensitive_url_components() {
        let endpoint = KevEndpoint::from_url(
            &Url::parse("https://user:password@example.test:8443/a/path?token=secret")
                .expect("test URL should parse"),
        );

        assert_eq!(endpoint.scheme, "https");
        assert_eq!(endpoint.host, "example.test");
        assert_eq!(endpoint.port, Some(8443));
        let logged_values = format!("{endpoint:?}");
        assert!(!logged_values.contains("user"));
        assert!(!logged_values.contains("password"));
        assert!(!logged_values.contains("/a/path"));
        assert!(!logged_values.contains("token=secret"));
    }

    #[test]
    fn proxy_environment_detects_uppercase_and_lowercase_variables_without_values() {
        let configured = ["http_proxy", "HTTPS_PROXY", "NO_PROXY"];
        let proxy_environment = ProxyEnvironment::from_lookup(|name| configured.contains(&name));

        assert_eq!(
            proxy_environment,
            ProxyEnvironment {
                http_proxy_configured: true,
                https_proxy_configured: true,
                no_proxy_configured: true,
            }
        );
    }

    #[test]
    fn kev_request_failure_class_uses_documented_precedence() {
        assert_eq!(
            KevRequestFailureClass::from_flags(ReqwestErrorFlags {
                timeout: true,
                connect: true,
                request: true,
                body: true,
                decode: true,
            }),
            KevRequestFailureClass::Timeout
        );
        assert_eq!(
            KevRequestFailureClass::from_flags(ReqwestErrorFlags {
                connect: true,
                request: true,
                body: true,
                decode: true,
                ..ReqwestErrorFlags::default()
            }),
            KevRequestFailureClass::Connect
        );
        assert_eq!(
            KevRequestFailureClass::from_flags(ReqwestErrorFlags {
                request: true,
                body: true,
                decode: true,
                ..ReqwestErrorFlags::default()
            }),
            KevRequestFailureClass::Request
        );
        assert_eq!(
            KevRequestFailureClass::from_flags(ReqwestErrorFlags {
                body: true,
                decode: true,
                ..ReqwestErrorFlags::default()
            }),
            KevRequestFailureClass::Body
        );
        assert_eq!(
            KevRequestFailureClass::from_flags(ReqwestErrorFlags {
                decode: true,
                ..ReqwestErrorFlags::default()
            }),
            KevRequestFailureClass::Decode
        );
        assert_eq!(
            KevRequestFailureClass::from_flags(ReqwestErrorFlags::default()),
            KevRequestFailureClass::Other
        );
    }

    #[test]
    fn kev_catalog_request_rejects_partial_content_response() {
        assert_kev_catalog_request_rejects_status(206, StatusCode::PARTIAL_CONTENT);
    }

    #[test]
    fn kev_catalog_request_rejects_server_error_response() {
        assert_kev_catalog_request_rejects_status(500, StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn kev_catalog_body_at_limit_is_accepted() {
        let mut body = Vec::new();
        append_kev_catalog_chunk(&mut body, &[1, 2, 3], 3)
            .expect("body at the configured limit should be accepted");

        assert_eq!(body, vec![1, 2, 3]);
    }

    #[test]
    fn kev_catalog_body_rejects_oversized_declared_length() {
        let error = validate_kev_catalog_content_length(Some(4), 3)
            .expect_err("an oversized declared response length should be rejected");

        assert!(matches!(
            error,
            KevError::CatalogResponseBodyTooLarge { max_bytes: 3 }
        ));
    }

    #[test]
    fn kev_catalog_body_rejects_oversized_chunked_response() {
        let mut body = Vec::new();
        append_kev_catalog_chunk(&mut body, &[1, 2], 3)
            .expect("first chunk should fit within the limit");
        let error = append_kev_catalog_chunk(&mut body, &[3, 4], 3)
            .expect_err("an oversized response without a declared length should be rejected");

        assert!(matches!(
            error,
            KevError::CatalogResponseBodyTooLarge { max_bytes: 3 }
        ));
    }

    #[test]
    fn fail_running_kev_sync_runs_marks_running_runs_failed() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_exec_results([MockExecResult {
                last_insert_id: 0,
                rows_affected: 2,
            }])
            .into_connection();

        let summary = run_async(fail_running_kev_sync_runs(
            &db,
            "sync process was interrupted",
        ))
        .expect("running sync runs should be marked failed");

        assert_eq!(
            summary,
            FailRunningKevSyncRunsSummary {
                sync_runs_failed: 2,
            }
        );
        let transaction_log = db.into_transaction_log();
        let update_sql = &transaction_log[0].statements()[0].sql;
        assert!(update_sql.contains(r#"UPDATE "public"."cisa_kev_sync_runs""#));
        assert!(update_sql.contains(r#""completed_at" = CURRENT_TIMESTAMP"#));
        assert!(update_sql.contains(r#""status" = $"#));
        assert!(update_sql.contains(r#""error" = $"#));
        assert!(update_sql.contains(r#"WHERE "cisa_kev_sync_runs"."status" = $"#));
    }

    #[test]
    fn reset_kev_storage_truncates_entries_and_sync_runs() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([
                vec![mock_advisory_lock_row(true)],
                vec![mock_num_items_row(0)],
                vec![mock_num_items_row(7)],
                vec![mock_num_items_row(3)],
            ])
            .append_exec_results([MockExecResult {
                last_insert_id: 0,
                rows_affected: 0,
            }])
            .into_connection();

        let summary = run_async(reset_kev_storage(&db, false)).expect("reset should succeed");

        assert_eq!(
            summary,
            ResetKevStorageSummary {
                entries_deleted: 7,
                sync_runs_deleted: 3,
            }
        );
        let transaction_log = db.into_transaction_log();
        assert!(transaction_log.iter().any(|entry| entry.statements()[0]
            .sql
            .contains("TRUNCATE TABLE public.cisa_kev_entries, public.cisa_kev_sync_runs RESTART IDENTITY")));
    }

    #[test]
    fn reset_kev_storage_rejects_when_sync_lock_is_held() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![mock_advisory_lock_row(false)]])
            .into_connection();

        let error = run_async(reset_kev_storage(&db, false)).expect_err("reset should fail");

        assert!(matches!(error, ResetKevStorageError::SyncAlreadyRunning));
        let transaction_log = db.into_transaction_log();
        assert!(
            transaction_log[0].statements()[0]
                .sql
                .contains("pg_try_advisory_xact_lock")
        );
        assert!(
            transaction_log
                .iter()
                .all(|entry| !entry.statements()[0].sql.contains("TRUNCATE TABLE"))
        );
    }

    #[test]
    fn reset_kev_storage_rejects_running_sync_runs_without_force() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([
                vec![mock_advisory_lock_row(true)],
                vec![mock_num_items_row(2)],
            ])
            .into_connection();

        let error = run_async(reset_kev_storage(&db, false)).expect_err("reset should fail");

        assert!(matches!(error, ResetKevStorageError::RunningSyncRuns(2)));
        let transaction_log = db.into_transaction_log();
        assert!(
            transaction_log
                .iter()
                .all(|entry| !entry.statements()[0].sql.contains("TRUNCATE TABLE"))
        );
    }

    #[test]
    fn reset_kev_storage_accepts_running_sync_runs_with_force() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([
                vec![mock_advisory_lock_row(true)],
                vec![mock_num_items_row(2)],
                vec![mock_num_items_row(7)],
                vec![mock_num_items_row(3)],
            ])
            .append_exec_results([MockExecResult {
                last_insert_id: 0,
                rows_affected: 0,
            }])
            .into_connection();

        let summary = run_async(reset_kev_storage(&db, true)).expect("reset should succeed");

        assert_eq!(
            summary,
            ResetKevStorageSummary {
                entries_deleted: 7,
                sync_runs_deleted: 3,
            }
        );
    }

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
            finish_kev_sync_run(
                &transaction,
                sync_run.generation,
                FinishKevSyncRun::failed("upstream request timed out".to_owned()),
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
            assert!(run.completed_at.is_some());
            assert_eq!(run.catalog_version, None);
            assert_eq!(run.catalog_date_released, None);
            assert_eq!(run.catalog_count, None);
            assert_eq!(run.content_sha256, None);
            assert_eq!(run.records_seen, 0);
            assert_eq!(run.records_inserted, 0);
            assert_eq!(run.records_updated, 0);

            cisa_kev_sync_runs::Entity::delete_many()
                .exec(&db)
                .await
                .expect("KEV sync runs should clear");
        });
    }

    #[test]
    fn store_all_entries_upserts_payload_with_change_aware_timestamps() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![mock_kev_entry_outcome(true, false)]])
            .into_connection();

        let summary = run_async(store_all_entries(
            &[kev_entry("initial description")],
            &db,
            slog::Logger::root(slog::Discard, slog::o!()),
        ))
        .expect("entry write should succeed");
        assert_eq!(summary.records_seen, 1);
        assert_eq!(summary.records_inserted, 1);
        assert_eq!(summary.records_updated, 0);

        let transaction_log = db.into_transaction_log();
        let upsert_sql = transaction_log
            .iter()
            .map(|entry| &entry.statements()[0].sql)
            .find(|sql| sql.contains(r#"INSERT INTO "public"."cisa_kev_entries""#))
            .expect("entry write should issue an insert");
        assert!(upsert_sql.contains("ON CONFLICT"));
        assert!(upsert_sql.contains(r#""entry" = EXCLUDED."entry""#));
        assert!(upsert_sql.contains(r#""last_seen_at" = CURRENT_TIMESTAMP"#));
        assert!(upsert_sql.contains(r#"IS DISTINCT FROM EXCLUDED."entry""#));
        assert!(upsert_sql.contains(r#"ELSE "cisa_kev_entries"."updated_at" END"#));
        assert!(upsert_sql.contains("RETURNING WITH (OLD AS old, NEW AS new)"));
        assert!(upsert_sql.contains("old.\"cve_id\" IS NULL AS inserted"));
        assert!(upsert_sql.contains("old.\"entry\" IS DISTINCT FROM new.\"entry\" AS updated"));
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
    fn store_all_entries_deduplicates_identical_cve_ids() {
        let entry = kev_entry("duplicate entry");
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![mock_kev_entry_outcome(true, false)]])
            .into_connection();

        let summary = run_async(store_all_entries(
            &[entry.clone(), entry],
            &db,
            slog::Logger::root(slog::Discard, slog::o!()),
        ))
        .expect("identical duplicate entries should be deduplicated");

        assert_eq!(summary.records_seen, 1);
        assert_eq!(summary.records_inserted, 1);
        assert_eq!(summary.records_updated, 0);
        assert_eq!(db.into_transaction_log().len(), 1);
    }

    #[test]
    fn store_all_entries_rejects_conflicting_duplicate_cve_ids_atomically() {
        let db = MockDatabase::new(DbBackend::Postgres).into_connection();

        let error = run_async(store_all_entries(
            &[
                kev_entry("first description"),
                kev_entry("conflicting description"),
            ],
            &db,
            slog::Logger::root(slog::Discard, slog::o!()),
        ))
        .expect_err("conflicting duplicate entries should fail the write");

        assert!(matches!(
            error,
            KevError::ConflictingDuplicateCveId(cve_id) if cve_id == "CVE-2026-1000"
        ));
        assert!(
            db.into_transaction_log().is_empty(),
            "a conflicting duplicate must prevent every database operation"
        );
    }

    #[test]
    fn catalog_count_mismatch_is_rejected_before_entry_storage() {
        let error = validate_catalog_count(2, 1)
            .expect_err("catalog count must match the number of vulnerability entries");

        assert!(matches!(
            error,
            KevError::CatalogCountMismatch {
                declared: 2,
                actual: 1
            }
        ));
    }

    #[test]
    fn negative_catalog_count_is_rejected_before_entry_storage() {
        let error = validate_catalog_count(-1, 0).expect_err("a catalog count cannot be negative");

        assert!(matches!(
            error,
            KevError::CatalogCountMismatch {
                declared: -1,
                actual: 0
            }
        ));
    }

    #[test]
    fn store_all_entries_counts_only_changed_existing_entries_as_updates() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![
                mock_kev_entry_outcome(false, true),
                mock_kev_entry_outcome(true, false),
            ]])
            .into_connection();

        let summary = run_async(store_all_entries(
            &[
                kev_entry("existing"),
                kev_entry_with_id("CVE-2026-1001", "new"),
            ],
            &db,
            slog::Logger::root(slog::Discard, slog::o!()),
        ))
        .expect("entry write should succeed");

        assert_eq!(summary.records_seen, 2);
        assert_eq!(summary.records_inserted, 1);
        assert_eq!(summary.records_updated, 1);
    }

    #[test]
    fn store_all_entries_does_not_count_unchanged_existing_entries_as_updates() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![mock_kev_entry_outcome(false, false)]])
            .into_connection();

        let summary = run_async(store_all_entries(
            &[kev_entry("unchanged")],
            &db,
            slog::Logger::root(slog::Discard, slog::o!()),
        ))
        .expect("entry write should succeed");

        assert_eq!(summary.records_seen, 1);
        assert_eq!(summary.records_inserted, 0);
        assert_eq!(summary.records_updated, 0);
    }

    #[test]
    fn sha256_hex_uses_lowercase_hex() {
        assert_eq!(
            sha256_hex(b"Night Vision"),
            "4cbe4ae9154d2b8ce3ad05bc9fd107420e66d7a62d465ccd5096ccd87cd7284d"
        );
    }

    #[test]
    fn finish_kev_sync_run_persists_terminal_metadata() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_exec_results([MockExecResult {
                last_insert_id: 0,
                rows_affected: 1,
            }])
            .into_connection();
        let completion = FinishKevSyncRun {
            status: KevSyncRunStatus::Success,
            catalog_version: Some("2026.08.20".to_owned()),
            catalog_date_released: Some("2026-08-20T00:00:00Z".parse().expect("valid date")),
            catalog_count: Some(2),
            content_sha256: Some("abc123".to_owned()),
            records_seen: 2,
            records_inserted: 1,
            records_updated: 1,
            error: None,
        };

        run_async(finish_kev_sync_run(&db, 42, completion))
            .expect("terminal metadata should be stored");

        let transaction_log = db.into_transaction_log();
        let sql = &transaction_log[0].statements()[0].sql;
        assert!(sql.contains(r#"UPDATE "public"."cisa_kev_sync_runs""#));
        assert!(sql.contains(r#""completed_at" = CURRENT_TIMESTAMP"#));
        assert!(sql.contains(r#""catalog_version" = $"#));
        assert!(sql.contains(r#""catalog_date_released" = $"#));
        assert!(sql.contains(r#""catalog_count" = $"#));
        assert!(sql.contains(r#""content_sha256" = $"#));
        assert!(sql.contains(r#""records_seen" = $"#));
        assert!(sql.contains(r#""records_inserted" = $"#));
        assert!(sql.contains(r#""records_updated" = $"#));
        assert!(sql.contains(r#""error" = $"#));
    }

    #[test]
    fn not_modified_completion_has_no_catalog_metadata() {
        let completion = FinishKevSyncRun::not_modified();

        assert_eq!(completion.status, KevSyncRunStatus::NotModified);
        assert_eq!(completion.records_seen, 0);
        assert_eq!(completion.records_inserted, 0);
        assert_eq!(completion.records_updated, 0);
        assert_eq!(completion.catalog_version, None);
        assert_eq!(completion.catalog_date_released, None);
        assert_eq!(completion.catalog_count, None);
        assert_eq!(completion.content_sha256, None);
    }

    #[test]
    fn failed_completion_has_no_catalog_metadata() {
        let completion = FinishKevSyncRun::failed("upstream request timed out".to_owned());

        assert_eq!(completion.status, KevSyncRunStatus::Failed);
        assert_eq!(
            completion.error.as_deref(),
            Some("upstream request timed out")
        );
        assert_eq!(completion.records_seen, 0);
        assert_eq!(completion.records_inserted, 0);
        assert_eq!(completion.records_updated, 0);
        assert_eq!(completion.catalog_version, None);
        assert_eq!(completion.catalog_date_released, None);
        assert_eq!(completion.catalog_count, None);
        assert_eq!(completion.content_sha256, None);
    }

    #[test]
    fn invalid_catalog_release_date_is_reported() {
        let error = "not-a-date"
            .parse::<sea_orm::prelude::DateTimeWithTimeZone>()
            .map_err(KevError::InvalidCatalogReleaseDate)
            .expect_err("invalid release date should fail");

        assert!(matches!(error, KevError::InvalidCatalogReleaseDate(_)));
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
            let initial_summary = write_entries(&db, std::slice::from_ref(&initial_entry)).await;
            assert_eq!(initial_summary.records_seen, 1);
            assert_eq!(initial_summary.records_inserted, 1);
            assert_eq!(initial_summary.records_updated, 0);
            let initial = read_entry(&db).await;

            tokio::time::sleep(Duration::from_millis(5)).await;
            let unchanged_summary = write_entries(&db, &[initial_entry]).await;
            assert_eq!(unchanged_summary.records_seen, 1);
            assert_eq!(unchanged_summary.records_inserted, 0);
            assert_eq!(unchanged_summary.records_updated, 0);
            let unchanged = read_entry(&db).await;
            assert_eq!(unchanged.entry, initial.entry);
            assert_eq!(unchanged.first_seen_at, initial.first_seen_at);
            assert!(unchanged.last_seen_at > initial.last_seen_at);
            assert_eq!(unchanged.updated_at, initial.updated_at);

            tokio::time::sleep(Duration::from_millis(5)).await;
            let changed_entry = kev_entry("changed description");
            let changed_summary = write_entries(&db, std::slice::from_ref(&changed_entry)).await;
            assert_eq!(changed_summary.records_seen, 1);
            assert_eq!(changed_summary.records_inserted, 0);
            assert_eq!(changed_summary.records_updated, 1);
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

    async fn write_entries(db: &DatabaseConnection, entries: &[serde_json::Value]) -> RunSummary {
        let transaction = db.begin().await.expect("transaction should begin");
        let summary = store_all_entries(
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
        summary
    }

    async fn read_entry(db: &DatabaseConnection) -> cisa_kev_entries::Model {
        cisa_kev_entries::Entity::find_by_id("CVE-2026-1000".to_owned())
            .one(db)
            .await
            .expect("entry lookup should succeed")
            .expect("entry should be stored")
    }

    fn kev_entry(short_description: &str) -> serde_json::Value {
        kev_entry_with_id("CVE-2026-1000", short_description)
    }

    fn kev_entry_with_id(cve_id: &str, short_description: &str) -> serde_json::Value {
        json!({
            "cveID": cve_id,
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

    fn mock_kev_entry_outcome(inserted: bool, updated: bool) -> BTreeMap<String, Value> {
        BTreeMap::from([
            ("inserted".to_owned(), inserted.into()),
            ("updated".to_owned(), updated.into()),
        ])
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

    fn assert_kev_catalog_request_rejects_status(status: u16, expected: StatusCode) {
        let server = MockServer::start();
        let _mock = server.mock(|when, then| {
            when.method(GET).path("/kev");
            then.status(status)
                .header("content-type", "application/json")
                .body(VALID_KEV_CATALOG);
        });
        let log = slog::Logger::root(slog::Discard, slog::o!());
        let url = Url::parse(&server.url("/kev")).expect("mock URL should be valid");
        let diagnostics = test_request_diagnostics(&url);

        let error = run_async(request_kev_catalog(
            &Client::new(),
            url,
            HeaderMap::new(),
            &diagnostics,
            &log,
        ))
        .expect_err("non-OK response should be rejected");

        assert!(matches!(
            error,
            KevError::UnexpectedResponseStatus(status) if status == expected
        ));
    }

    fn test_request_diagnostics(url: &Url) -> KevRequestDiagnostics {
        KevRequestDiagnostics {
            generation: 42,
            conditional_request: false,
            endpoint: KevEndpoint::from_url(url),
            proxy_environment: ProxyEnvironment {
                http_proxy_configured: false,
                https_proxy_configured: false,
                no_proxy_configured: false,
            },
        }
    }

    fn run_async<T>(future: impl std::future::Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime should build")
            .block_on(future)
    }
}
