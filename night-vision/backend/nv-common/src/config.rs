//! Defines configuration for the Night Vision server.

use crate::{
    cve::{
        git::{DEFAULT_CVE_RECORD_MAX_BYTES, GitRef},
        repository::DEFAULT_CVE_RECORD_PARSE_CONCURRENCY,
        storage::{
            DEFAULT_CVE_LIST_RECORD_WRITE_BATCH_SIZE, DEFAULT_CVE_LIST_RECORD_WRITE_CHANNEL_SIZE,
        },
        sync::{CveListSyncPipelineConfig, CveListSyncTimeoutConfig},
    },
    error::ErrorSourceIterator as _,
    hipcheck::HipcheckRunnerConfig,
    npm::elaboration::ElaborationLimits,
    secret::{SecretFileError, SecretSource, SecretSourceKind},
};
use camino::{Utf8Path, Utf8PathBuf};
use dropshot::{ConfigDropshot, HandlerTaskMode};
use itertools::Itertools as _;
use secrecy::SecretString;
use std::{
    collections::BTreeMap,
    error::Error as _,
    ffi::OsString,
    fmt::{Debug, Display},
    fs::File,
    io::BufReader,
    net::AddrParseError,
    str::FromStr,
    time::Duration,
};
use url::Url;

/// The default configuration file, relative to the root of the workspace. This is used by the CLI
/// if `-c`/`--config` is not set.
pub const DEFAULT_CONFIG_FILE: &str = "nv-server.spookey";

const DEFAULT_CVE_LIST_REPOSITORY_URL: &str = "https://github.com/CVEProject/cvelistV5.git";
const DEFAULT_CVE_LIST_REPOSITORY_REF: &str = "main";
const DEFAULT_CVE_LIST_SYNC_INTERVAL: u64 = 420_000;
const DEFAULT_CVE_LIST_SYNC_TIMEOUT: u64 = 3_600_000;
const DEFAULT_CVE_LIST_FIRST_SYNC_TIMEOUT: u64 = 14_400_000;
const DEFAULT_CVE_LIST_PARSE_CONCURRENCY_CAP: usize = DEFAULT_CVE_RECORD_PARSE_CONCURRENCY;
const DEFAULT_CVE_LIST_WRITE_BATCH_SIZE: usize = DEFAULT_CVE_LIST_RECORD_WRITE_BATCH_SIZE;
const DEFAULT_CVE_LIST_WRITE_CHANNEL_SIZE_CAP: usize = DEFAULT_CVE_LIST_RECORD_WRITE_CHANNEL_SIZE;
const DEFAULT_ASYNC_MAX_BLOCKING_THREADS: usize = 512;
const MIN_COMPUTED_CVE_LIST_PARSE_CONCURRENCY: usize = 2;
const DEFAULT_NPM_REGISTRY_URL: &str = "https://registry.npmjs.org/";
const DEFAULT_PACKAGE_ELABORATION_MAX_PACKUMENT_BYTES: usize = 64 * 1024 * 1024;
const DEFAULT_PACKAGE_ELABORATION_REQUEST_TIMEOUT: u64 = 30_000;
const DEFAULT_PACKAGE_ELABORATION_TOTAL_RUN_TIMEOUT: u64 = 300_000;
const DEFAULT_PACKAGE_ELABORATION_MAX_PACKAGES: usize = 10_000;
const DEFAULT_PACKAGE_ELABORATION_MAX_EDGES: usize = 1_000_000;
const DEFAULT_PACKAGE_ELABORATION_MAX_QUEUED_WORK: usize = 100_000;
const DEFAULT_PACKAGE_ELABORATION_MAX_DERIVATIONS: usize = 1_000_000;
const DEFAULT_PACKAGE_ELABORATION_MAX_CONCURRENT_RUNS: usize = 4;
const DEFAULT_HIPCHECK_BIN_PATH: &str = "/usr/local/bin/hc";
const HIPCHECK_SYSTEM_PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";
const DEFAULT_HIPCHECK_POLICY_PATH: &str = "/opt/night-vision/hipcheck/config/Hipcheck.kdl";
const DEFAULT_HIPCHECK_EXEC_CONFIG_PATH: &str = "/opt/night-vision/hipcheck/config/Exec.kdl";
const DEFAULT_HIPCHECK_WORKING_DIR: &str = "/var/cache/night-vision/hipcheck";
const DEFAULT_HIPCHECK_CACHE_DIR: &str = "/var/cache/night-vision/hipcheck";
const DEFAULT_HIPCHECK_TIMEOUT: u64 = 300_000;
const DEFAULT_HIPCHECK_STDOUT_MAX_BYTES: usize = 1024 * 1024;
const DEFAULT_HIPCHECK_STDERR_MAX_BYTES: usize = 1024 * 1024;
const DEFAULT_HIPCHECK_JSON_MAX_BYTES: usize = 4 * 1024 * 1024;
const DEFAULT_HIPCHECK_MAX_CONCURRENT_RUNS: usize = 1;

/// Errors that can occur while loading configuration.
pub enum ConfigLoadError {
    FailedToOpenConfigFile(Utf8PathBuf, std::io::Error),
    FailedToParseConfigFile(Utf8PathBuf, spookey::Error),
    FailedToParseConfigFileFields(Utf8PathBuf, ConfigErrors),
    FailedToReadSecretFile(Utf8PathBuf, SecretFileError),
}

// The `Debug` representation is intended to match the debug printing for `anyhow::Error`, with a
// top-level error and then a series of causes.
impl Debug for ConfigLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut msg = format!("{self}\n");

        if let Some(source) = self.source() {
            msg.push_str("\nCaused by:\n");

            for source in source.sources_iter() {
                use std::fmt::Write as _;

                let _ = writeln!(msg, "\t{source}");
            }
        }

        write!(f, "{msg}")
    }
}

impl Display for ConfigLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FailedToOpenConfigFile(path, _) => {
                write!(f, "failed to open config file '{path}'")
            }
            Self::FailedToParseConfigFile(path, _) => {
                write!(f, "failed to parse config file '{path}'")
            }
            Self::FailedToParseConfigFileFields(path, _) => {
                write!(f, "failed to parse config file '{path}'")
            }
            Self::FailedToReadSecretFile(path, _) => {
                write!(f, "failed to read secret file '{path}'")
            }
        }
    }
}

impl std::error::Error for ConfigLoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::FailedToOpenConfigFile(_, err) => Some(err),
            Self::FailedToParseConfigFile(_, err) => Some(err),
            Self::FailedToParseConfigFileFields(_, err) => Some(err),
            Self::FailedToReadSecretFile(_, err) => Some(err),
        }
    }
}

/// Configuration for the Night Vision server.
#[derive(Debug)]
pub struct Config {
    /// The path to the configuration file.
    config_file_path: Utf8PathBuf,

    /// The address to bind the server to.
    ///
    /// The default is `127.0.0.1` at an arbitrary available port..
    pub server_address: String,

    /// The path to the OpenAPI Definition file to write on startup.
    /// If unset, will not write out the OpenAPI Definition.
    pub openapi_dest_path: Option<Utf8PathBuf>,

    /// The URL from which Known Exploited Vulnerabilities (KEV) data
    /// should be fetched.
    pub kev_url: Option<reqwest::Url>,

    /// The interval in milliseconds between refreshes of KEV data from its source URL.
    pub kev_refresh_interval: Option<u64>,

    /// The maximum size in bytes of a KEV catalog response body.
    pub kev_response_body_max_bytes: Option<usize>,

    /// The maximum size in bytes for the request body.
    ///
    /// The default is 1024 bytes.
    pub http_request_body_max_bytes: Option<usize>,

    /// The default behavior for HTTP handler functions when clients disconnect early.
    pub http_early_disconnect_behavior: Option<EarlyDisconnectBehavior>,

    /// Source for the string used to connect to the database.
    database_connection_source: SecretSourceKind,

    /// String used to connect to the database.
    database_connection: SecretString,

    /// Optional bearer token required to read operator health diagnostics.
    health_diagnostics_token: Option<SecretString>,

    /// The maximum number of connections to the database.
    pub database_max_connections: Option<u32>,

    /// The minimum number of connections to the database.
    pub database_min_connections: Option<u32>,

    /// The timeout in milliseconds for establishing a connection to the database.
    pub database_connect_timeout: Option<u64>,

    /// The timeout in milliseconds for idle connections to the database.
    pub database_idle_timeout: Option<u64>,

    /// The timeout in milliseconds for acquiring a connection from the database.
    pub database_acquire_timeout: Option<u64>,

    /// The maximum lifetime in milliseconds for a connection to the database.
    pub database_max_lifetime: Option<u64>,

    /// The number of async worker threads for Tokio to use.
    ///
    /// The default is equal to the number of CPU cores available on the system.
    pub async_worker_threads: Option<usize>,

    /// The size in bytes for stacks available to worker tasks.
    ///
    /// The default is 2MB.
    pub async_worker_thread_stack_size: Option<usize>,

    /// The maximum number of additional blocking threads the async runtime can spawn.
    ///
    /// These "blocking threads" are used to offload blocking I/O operations from the async runtime,
    /// for example: filesystem operations, DNS resolution, writing to stdout/stderr, reading from
    /// stdin.
    ///
    /// The default is 512.
    pub async_max_blocking_threads: Option<usize>,

    /// The number of milliseconds to keep blocking threads alive after they have finished processing.
    ///
    /// The default is 10,000 (10 seconds).
    pub async_blocking_thread_keep_alive: Option<u64>,

    /// The number of scheduler ticks between polls to the global task queue.
    ///
    /// Worker threads have their own local queues of tasks, but will also periodically check the
    /// global queue for tasks as well. This setting controls how often that poll occurs.
    ///
    /// The default is 31 ticks.
    pub async_global_queue_interval: Option<u32>,

    /// The number of scheduler ticks between polls for external events.
    ///
    /// This determines how often the async runtime will poll for external events, such as network
    /// I/O or file system notifications.
    ///
    /// The default is 61 ticks.
    pub async_event_interval: Option<u32>,

    /// The Git repository URL to fetch CVE List data from.
    pub cve_list_repository_url: Url,

    /// The Git repository ref to fetch CVE List data from.
    pub cve_list_repository_ref: GitRef,

    /// The interval in milliseconds between CVE List sync attempts.
    pub cve_list_sync_interval: u64,

    /// The timeout in milliseconds for one CVE List sync attempt.
    pub cve_list_sync_timeout: u64,

    /// The timeout in milliseconds for the first CVE List sync attempt.
    pub cve_list_first_sync_timeout: u64,

    /// The local checkout path for the CVE List repository cache.
    pub cve_list_checkout_path: Utf8PathBuf,

    /// The maximum number of CVE List records to parse concurrently.
    pub cve_list_parse_concurrency: usize,

    /// The maximum number of CVE List records to write in one database batch.
    pub cve_list_write_batch_size: usize,

    /// The maximum size in bytes for one CVE List record blob.
    pub cve_record_max_bytes: usize,

    /// The number of parsed CVE List records to buffer before database writes.
    pub cve_list_write_channel_size: usize,

    /// Whether CVE List parse concurrency was explicit or computed.
    pub cve_list_parse_concurrency_source: ConfigValueSource,

    /// Whether CVE List write batch size was explicit or defaulted.
    pub cve_list_write_batch_size_source: ConfigValueSource,

    /// Whether CVE List write channel size was explicit or computed.
    pub cve_list_write_channel_size_source: ConfigValueSource,

    /// The NPM registry used for package-source elaboration.
    pub npm_registry_url: Url,

    /// Bounded concurrency and result limits for package-source elaboration.
    pub package_elaboration_limits: ElaborationLimits,

    /// The maximum accepted NPM packument response size in bytes.
    pub package_elaboration_max_packument_bytes: usize,

    /// The maximum number of package-source elaborations running at once.
    pub package_elaboration_max_concurrent_runs: usize,

    /// Process-boundary configuration for bundled Hipcheck assessments.
    pub hipcheck_bin_path: Utf8PathBuf,
    pub hipcheck_policy_path: Utf8PathBuf,
    pub hipcheck_exec_config_path: Utf8PathBuf,
    pub hipcheck_working_dir: Utf8PathBuf,
    pub hipcheck_cache_dir: Utf8PathBuf,
    pub hipcheck_data_dir: Utf8PathBuf,
    pub hipcheck_timeout: u64,
    pub hipcheck_stdout_max_bytes: usize,
    pub hipcheck_stderr_max_bytes: usize,
    pub hipcheck_json_max_bytes: usize,
    pub hipcheck_max_concurrent_runs: usize,
}

/// Where a resolved configuration value came from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigValueSource {
    /// The value was set in the configuration file.
    Explicit,
    /// The value was computed because the configuration key was unset.
    ComputedDefault,
}

impl ConfigValueSource {
    /// Whether the value came from an automatic default.
    pub fn is_computed_default(self) -> bool {
        matches!(self, Self::ComputedDefault)
    }
}

impl Display for ConfigValueSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Explicit => write!(f, "explicit"),
            Self::ComputedDefault => write!(f, "computed_default"),
        }
    }
}

/// Runtime configuration for the CVE List worker.
#[derive(Clone, Debug)]
pub struct CveListWorkerConfig {
    repository_url: Url,
    repository_ref: GitRef,
    sync_interval: Duration,
    sync_timeout: Duration,
    first_sync_timeout: Duration,
    checkout_path: Utf8PathBuf,
    parse_concurrency: usize,
    write_batch_size: usize,
    record_max_bytes: usize,
    write_channel_size: usize,
    parse_concurrency_source: ConfigValueSource,
    write_batch_size_source: ConfigValueSource,
    write_channel_size_source: ConfigValueSource,
}

impl CveListWorkerConfig {
    /// The Git repository URL to fetch CVE List data from.
    pub fn repository_url(&self) -> &Url {
        &self.repository_url
    }

    /// The Git repository ref to fetch CVE List data from.
    pub fn repository_ref(&self) -> &GitRef {
        &self.repository_ref
    }

    /// The interval between CVE List sync attempts.
    pub fn sync_interval(&self) -> Duration {
        self.sync_interval
    }

    /// The timeout for one CVE List sync attempt.
    pub fn sync_timeout(&self) -> Duration {
        self.sync_timeout
    }

    /// The timeout for the first CVE List sync attempt.
    pub fn first_sync_timeout(&self) -> Duration {
        self.first_sync_timeout
    }

    /// The local checkout path for the CVE List repository cache.
    pub fn checkout_path(&self) -> &Utf8Path {
        &self.checkout_path
    }

    /// The maximum number of CVE List records to parse concurrently.
    pub fn parse_concurrency(&self) -> usize {
        self.parse_concurrency
    }

    /// The maximum number of CVE List records to write in one database batch.
    pub fn write_batch_size(&self) -> usize {
        self.write_batch_size
    }

    /// The maximum size in bytes for one CVE List record blob.
    pub fn record_max_bytes(&self) -> usize {
        self.record_max_bytes
    }

    /// The number of parsed CVE List records to buffer before database writes.
    pub fn write_channel_size(&self) -> usize {
        self.write_channel_size
    }

    /// Whether parse concurrency was explicit or computed.
    pub fn parse_concurrency_source(&self) -> ConfigValueSource {
        self.parse_concurrency_source
    }

    /// Whether write batch size was explicit or defaulted.
    pub fn write_batch_size_source(&self) -> ConfigValueSource {
        self.write_batch_size_source
    }

    /// Whether write channel size was explicit or computed.
    pub fn write_channel_size_source(&self) -> ConfigValueSource {
        self.write_channel_size_source
    }

    /// Build pipeline tuning for one CVE List sync attempt.
    pub fn sync_pipeline_config(&self) -> CveListSyncPipelineConfig {
        CveListSyncPipelineConfig {
            write_batch_size: self.write_batch_size,
            parse_concurrency: self.parse_concurrency,
            write_channel_size: self.write_channel_size,
        }
    }

    /// Build timeout policy for one CVE List sync attempt.
    pub fn sync_timeout_config(&self) -> CveListSyncTimeoutConfig {
        CveListSyncTimeoutConfig {
            sync_timeout: self.sync_timeout,
            first_sync_timeout: self.first_sync_timeout,
        }
    }
}

impl Config {
    /// Parse configuration from a configuration file.
    ///
    /// If not provided, then `nv-server` will look for a file called `nv-server.spookey` in the
    /// current directory.
    ///
    /// Parsing will fail on unknown keys, or on keys that fail to parse. This is purposefully
    /// pretty strict; these are server configuration items, and a malformed key or value should
    /// be considered a configuration failure.
    pub fn parse(path: &Utf8Path) -> Result<Self, ConfigLoadError> {
        let parsed = spookey::parse(
            spookey::ParseConfig {
                // While we *do* require some form of database connection information, either via
                // `database-connection` or `database-connection-file`, they're both listed as
                // optional here because we validate that only one of them is present after
                // parsing with `spookey`.
                required_keys: vec!["server-address", "cve-list-checkout-path"],
                optional_keys: vec![
                    "openapi-dest-path",
                    "kev-url",
                    "kev-refresh-interval",
                    "kev-response-body-max-bytes",
                    "http-request-body-max-bytes",
                    "http-early-disconnect-behavior",
                    "database-connection",
                    "database-connection-file",
                    "health-diagnostics-token-file",
                    "database-max-connections",
                    "database-min-connections",
                    "database-connect-timeout",
                    "database-idle-timeout",
                    "database-acquire-timeout",
                    "database-max-lifetime",
                    "async-worker-threads",
                    "async-worker-thread-stack-size",
                    "async-max-blocking-threads",
                    "async-blocking-thread-keep-alive",
                    "async-global-queue-interval",
                    "async-event-interval",
                    "cve-list-repository-url",
                    "cve-list-repository-ref",
                    "cve-list-sync-interval",
                    "cve-list-sync-timeout",
                    "cve-list-first-sync-timeout",
                    "cve-list-parse-concurrency",
                    "cve-list-write-batch-size",
                    "cve-record-max-bytes",
                    "cve-list-write-channel-size",
                    "npm-registry-url",
                    "package-elaboration-worker-concurrency",
                    "package-elaboration-work-queue-capacity",
                    "package-elaboration-max-packument-bytes",
                    "package-elaboration-request-timeout",
                    "package-elaboration-total-run-timeout",
                    "package-elaboration-max-packages",
                    "package-elaboration-max-edges",
                    "package-elaboration-max-queued-work",
                    "package-elaboration-max-derivations",
                    "package-elaboration-max-concurrent-runs",
                    "hipcheck-bin-path",
                    "hipcheck-policy-path",
                    "hipcheck-exec-config-path",
                    "hipcheck-working-dir",
                    "hipcheck-cache-dir",
                    "hipcheck-data-dir",
                    "hipcheck-timeout",
                    "hipcheck-stdout-max-bytes",
                    "hipcheck-stderr-max-bytes",
                    "hipcheck-json-max-bytes",
                    "hipcheck-max-concurrent-runs",
                ],
            },
            BufReader::new(
                File::open(path)
                    .map_err(|e| ConfigLoadError::FailedToOpenConfigFile(path.into(), e))?,
            ),
        )
        .map_err(|e| ConfigLoadError::FailedToParseConfigFile(path.into(), e))?;

        // Construct the config object, parsing fields into the proper types and collecting errors.
        let mut errors = Vec::new();
        let database_connection =
            parse_value::<String>(&parsed, "database-connection", &mut errors)
                .map(SecretString::from);
        let database_connection_file =
            parse_value(&parsed, "database-connection-file", &mut errors);
        let database_connection_source = parse_database_connection_source(
            database_connection,
            database_connection_file,
            &mut errors,
        );
        let health_diagnostics_token_file =
            parse_value(&parsed, "health-diagnostics-token-file", &mut errors);

        let server_address: String = parse_value(&parsed, "server-address", &mut errors)
            .expect("server-address is required");
        let openapi_dest_path: Option<Utf8PathBuf> =
            parse_value(&parsed, "openapi-dest-path", &mut errors);
        let kev_url: Option<reqwest::Url> = parse_value(&parsed, "kev-url", &mut errors);
        let kev_refresh_interval =
            parse_kev_refresh_interval(&parsed, "kev-refresh-interval", &mut errors);
        let kev_response_body_max_bytes =
            parse_positive_usize_option(&parsed, "kev-response-body-max-bytes", &mut errors);
        let http_request_body_max_bytes =
            parse_value(&parsed, "http-request-body-max-bytes", &mut errors);
        let http_early_disconnect_behavior =
            parse_value(&parsed, "http-early-disconnect-behavior", &mut errors);
        let database_max_connections =
            parse_value(&parsed, "database-max-connections", &mut errors);
        let database_min_connections =
            parse_value(&parsed, "database-min-connections", &mut errors);
        let database_connect_timeout =
            parse_value(&parsed, "database-connect-timeout", &mut errors);
        let database_idle_timeout = parse_value(&parsed, "database-idle-timeout", &mut errors);
        let database_acquire_timeout =
            parse_value(&parsed, "database-acquire-timeout", &mut errors);
        let database_max_lifetime = parse_value(&parsed, "database-max-lifetime", &mut errors);
        let async_worker_threads = parse_value(&parsed, "async-worker-threads", &mut errors);
        let async_worker_thread_stack_size =
            parse_value(&parsed, "async-worker-thread-stack-size", &mut errors);
        let async_max_blocking_threads =
            parse_value(&parsed, "async-max-blocking-threads", &mut errors);
        let async_blocking_thread_keep_alive =
            parse_value(&parsed, "async-blocking-thread-keep-alive", &mut errors);
        let async_global_queue_interval =
            parse_value(&parsed, "async-global-queue-interval", &mut errors);
        let async_event_interval = parse_value(&parsed, "async-event-interval", &mut errors);
        let cve_list_repository_url = parse_cve_list_repository_url(&parsed, &mut errors)
            .unwrap_or_else(default_cve_list_repository_url);
        let cve_list_repository_ref = parse_value(&parsed, "cve-list-repository-ref", &mut errors)
            .unwrap_or_else(default_cve_list_repository_ref);
        let cve_list_sync_interval = parse_positive_u64(
            &parsed,
            "cve-list-sync-interval",
            DEFAULT_CVE_LIST_SYNC_INTERVAL,
            &mut errors,
        );
        let cve_list_sync_timeout = parse_positive_u64(
            &parsed,
            "cve-list-sync-timeout",
            DEFAULT_CVE_LIST_SYNC_TIMEOUT,
            &mut errors,
        );
        let cve_list_first_sync_timeout = parse_positive_u64(
            &parsed,
            "cve-list-first-sync-timeout",
            DEFAULT_CVE_LIST_FIRST_SYNC_TIMEOUT,
            &mut errors,
        );
        let cve_list_checkout_path: Utf8PathBuf =
            parse_value(&parsed, "cve-list-checkout-path", &mut errors)
                .expect("cve-list-checkout-path is required");
        let cve_list_parse_concurrency =
            parse_positive_usize_option(&parsed, "cve-list-parse-concurrency", &mut errors);
        let cve_list_write_batch_size =
            parse_positive_usize_option(&parsed, "cve-list-write-batch-size", &mut errors);
        let cve_record_max_bytes = parse_positive_usize(
            &parsed,
            "cve-record-max-bytes",
            DEFAULT_CVE_RECORD_MAX_BYTES,
            &mut errors,
        );
        let cve_list_write_channel_size =
            parse_positive_usize_option(&parsed, "cve-list-write-channel-size", &mut errors);
        let npm_registry_url = parse_value(&parsed, "npm-registry-url", &mut errors)
            .unwrap_or_else(default_npm_registry_url);
        let package_elaboration_worker_concurrency = parse_positive_usize_option(
            &parsed,
            "package-elaboration-worker-concurrency",
            &mut errors,
        );
        let package_elaboration_work_queue_capacity = parse_positive_usize_option(
            &parsed,
            "package-elaboration-work-queue-capacity",
            &mut errors,
        );
        let package_elaboration_max_packument_bytes = parse_positive_usize(
            &parsed,
            "package-elaboration-max-packument-bytes",
            DEFAULT_PACKAGE_ELABORATION_MAX_PACKUMENT_BYTES,
            &mut errors,
        );
        let package_elaboration_request_timeout = parse_positive_u64(
            &parsed,
            "package-elaboration-request-timeout",
            DEFAULT_PACKAGE_ELABORATION_REQUEST_TIMEOUT,
            &mut errors,
        );
        let package_elaboration_total_run_timeout = parse_positive_u64(
            &parsed,
            "package-elaboration-total-run-timeout",
            DEFAULT_PACKAGE_ELABORATION_TOTAL_RUN_TIMEOUT,
            &mut errors,
        );
        let package_elaboration_max_packages = parse_positive_usize(
            &parsed,
            "package-elaboration-max-packages",
            DEFAULT_PACKAGE_ELABORATION_MAX_PACKAGES,
            &mut errors,
        );
        let package_elaboration_max_derivations = parse_positive_usize(
            &parsed,
            "package-elaboration-max-derivations",
            DEFAULT_PACKAGE_ELABORATION_MAX_DERIVATIONS,
            &mut errors,
        );
        let package_elaboration_max_edges = parse_positive_usize(
            &parsed,
            "package-elaboration-max-edges",
            DEFAULT_PACKAGE_ELABORATION_MAX_EDGES,
            &mut errors,
        );
        let package_elaboration_max_queued_work = parse_positive_usize(
            &parsed,
            "package-elaboration-max-queued-work",
            DEFAULT_PACKAGE_ELABORATION_MAX_QUEUED_WORK,
            &mut errors,
        );
        let package_elaboration_max_concurrent_runs = parse_positive_usize(
            &parsed,
            "package-elaboration-max-concurrent-runs",
            DEFAULT_PACKAGE_ELABORATION_MAX_CONCURRENT_RUNS,
            &mut errors,
        );
        let hipcheck_bin_path = parse_value(&parsed, "hipcheck-bin-path", &mut errors)
            .unwrap_or_else(|| Utf8PathBuf::from(DEFAULT_HIPCHECK_BIN_PATH));
        let hipcheck_policy_path = parse_value(&parsed, "hipcheck-policy-path", &mut errors)
            .unwrap_or_else(|| Utf8PathBuf::from(DEFAULT_HIPCHECK_POLICY_PATH));
        let hipcheck_exec_config_path =
            parse_value(&parsed, "hipcheck-exec-config-path", &mut errors)
                .unwrap_or_else(|| Utf8PathBuf::from(DEFAULT_HIPCHECK_EXEC_CONFIG_PATH));
        let hipcheck_working_dir = parse_value(&parsed, "hipcheck-working-dir", &mut errors)
            .unwrap_or_else(|| Utf8PathBuf::from(DEFAULT_HIPCHECK_WORKING_DIR));
        let hipcheck_cache_dir = parse_value(&parsed, "hipcheck-cache-dir", &mut errors)
            .unwrap_or_else(|| Utf8PathBuf::from(DEFAULT_HIPCHECK_CACHE_DIR));
        let hipcheck_data_dir = parse_value(&parsed, "hipcheck-data-dir", &mut errors)
            .unwrap_or_else(|| Utf8PathBuf::from("/var/lib/night-vision/hipcheck"));
        let hipcheck_timeout = parse_positive_u64(
            &parsed,
            "hipcheck-timeout",
            DEFAULT_HIPCHECK_TIMEOUT,
            &mut errors,
        );
        let hipcheck_stdout_max_bytes = parse_positive_usize(
            &parsed,
            "hipcheck-stdout-max-bytes",
            DEFAULT_HIPCHECK_STDOUT_MAX_BYTES,
            &mut errors,
        );
        let hipcheck_stderr_max_bytes = parse_positive_usize(
            &parsed,
            "hipcheck-stderr-max-bytes",
            DEFAULT_HIPCHECK_STDERR_MAX_BYTES,
            &mut errors,
        );
        let hipcheck_json_max_bytes = parse_positive_usize(
            &parsed,
            "hipcheck-json-max-bytes",
            DEFAULT_HIPCHECK_JSON_MAX_BYTES,
            &mut errors,
        );
        let hipcheck_max_concurrent_runs = parse_positive_usize(
            &parsed,
            "hipcheck-max-concurrent-runs",
            DEFAULT_HIPCHECK_MAX_CONCURRENT_RUNS,
            &mut errors,
        );

        check_errors(path, parsed.warnings, errors)?;

        let (database_connection_source, database_connection) =
            resolve_database_connection_source(database_connection_source)?;
        let health_diagnostics_token = resolve_optional_secret_file(health_diagnostics_token_file)?;
        let cve_list_pipeline_config = resolve_cve_list_pipeline_config(
            async_worker_threads,
            async_max_blocking_threads,
            cve_list_parse_concurrency,
            cve_list_write_batch_size,
            cve_list_write_channel_size,
        );
        let package_elaboration_limits = resolve_package_elaboration_limits(
            async_worker_threads,
            PackageElaborationLimitSettings {
                worker_concurrency: package_elaboration_worker_concurrency,
                work_queue_capacity: package_elaboration_work_queue_capacity,
                request_timeout: package_elaboration_request_timeout,
                total_run_timeout: package_elaboration_total_run_timeout,
                max_packages: package_elaboration_max_packages,
                max_edges: package_elaboration_max_edges,
                max_queued_work: package_elaboration_max_queued_work,
                max_derivations: package_elaboration_max_derivations,
            },
        );

        let config = Self {
            config_file_path: path.into(),
            server_address,
            openapi_dest_path,
            kev_url,
            kev_refresh_interval,
            kev_response_body_max_bytes,
            http_request_body_max_bytes,
            http_early_disconnect_behavior,
            database_connection_source,
            database_connection,
            health_diagnostics_token,
            database_max_connections,
            database_min_connections,
            database_connect_timeout,
            database_idle_timeout,
            database_acquire_timeout,
            database_max_lifetime,
            async_worker_threads,
            async_worker_thread_stack_size,
            async_max_blocking_threads,
            async_blocking_thread_keep_alive,
            async_global_queue_interval,
            async_event_interval,
            cve_list_repository_url,
            cve_list_repository_ref,
            cve_list_sync_interval,
            cve_list_sync_timeout,
            cve_list_first_sync_timeout,
            cve_list_checkout_path,
            cve_list_parse_concurrency: cve_list_pipeline_config.parse_concurrency,
            cve_list_write_batch_size: cve_list_pipeline_config.write_batch_size,
            cve_record_max_bytes,
            cve_list_write_channel_size: cve_list_pipeline_config.write_channel_size,
            cve_list_parse_concurrency_source: cve_list_pipeline_config.parse_concurrency_source,
            cve_list_write_batch_size_source: cve_list_pipeline_config.write_batch_size_source,
            cve_list_write_channel_size_source: cve_list_pipeline_config.write_channel_size_source,
            npm_registry_url,
            package_elaboration_limits,
            package_elaboration_max_packument_bytes,
            package_elaboration_max_concurrent_runs,
            hipcheck_bin_path,
            hipcheck_policy_path,
            hipcheck_exec_config_path,
            hipcheck_working_dir,
            hipcheck_cache_dir,
            hipcheck_data_dir,
            hipcheck_timeout,
            hipcheck_stdout_max_bytes,
            hipcheck_stderr_max_bytes,
            hipcheck_json_max_bytes,
            hipcheck_max_concurrent_runs,
        };

        Ok(config)
    }

    /// Get the Dropshot server configuration items out of the overall config.
    ///
    /// The `ConfigDropshot` type is the type that Dropshot expects for its own configuration,
    /// so we take the fields that apply to Dropshot and pull them out here.
    pub fn dropshot_config(&self) -> Result<ConfigDropshot, ConfigLoadError> {
        let mut config = ConfigDropshot {
            bind_address: self.server_address.parse().map_err(|e: AddrParseError| {
                ConfigLoadError::FailedToParseConfigFileFields(
                    self.config_file_path.clone(),
                    ConfigErrors(vec![ConfigError::StrParse(StrParseError {
                        key: "server-address".to_owned().into_boxed_str(),
                        value: self.server_address.clone().into_boxed_str(),
                        err: e.to_string().into_boxed_str(),
                    })]),
                )
            })?,
            ..Default::default()
        };

        if let Some(max_bytes) = self.http_request_body_max_bytes {
            config.default_request_body_max_bytes = max_bytes;
        }

        if let Some(behavior) = self.http_early_disconnect_behavior {
            config.default_handler_task_mode = behavior.into();
        }

        Ok(config)
    }

    /// Get the configured database connection string.
    pub fn database_connection(&self) -> &SecretString {
        &self.database_connection
    }

    /// Get the optional token that enables operator health diagnostics.
    pub fn health_diagnostics_token(&self) -> Option<&SecretString> {
        self.health_diagnostics_token.as_ref()
    }

    /// Build the deliberately hermetic Hipcheck process configuration.
    pub fn hipcheck_runner_config(&self) -> HipcheckRunnerConfig {
        let mut environment = BTreeMap::new();
        environment.insert(OsString::from("PATH"), OsString::from(HIPCHECK_SYSTEM_PATH));
        environment.insert(
            OsString::from("TMPDIR"),
            OsString::from(self.hipcheck_cache_dir.join("tmp").as_str()),
        );

        HipcheckRunnerConfig {
            program: self.hipcheck_bin_path.clone(),
            working_directory: self.hipcheck_working_dir.clone(),
            policy_path: self.hipcheck_policy_path.clone(),
            exec_config_path: self.hipcheck_exec_config_path.clone(),
            cache_directory: self.hipcheck_cache_dir.clone(),
            environment,
            timeout: Duration::from_millis(self.hipcheck_timeout),
            stdout_max_bytes: self.hipcheck_stdout_max_bytes,
            stderr_max_bytes: self.hipcheck_stderr_max_bytes,
            json_max_bytes: self.hipcheck_json_max_bytes,
        }
    }

    /// Build runtime configuration for the CVE List worker.
    pub fn cve_list_worker_config(&self) -> CveListWorkerConfig {
        CveListWorkerConfig {
            repository_url: self.cve_list_repository_url.clone(),
            repository_ref: self.cve_list_repository_ref.clone(),
            sync_interval: Duration::from_millis(self.cve_list_sync_interval),
            sync_timeout: Duration::from_millis(self.cve_list_sync_timeout),
            first_sync_timeout: Duration::from_millis(self.cve_list_first_sync_timeout),
            checkout_path: self.cve_list_checkout_path.clone(),
            parse_concurrency: self.cve_list_parse_concurrency,
            write_batch_size: self.cve_list_write_batch_size,
            record_max_bytes: self.cve_record_max_bytes,
            write_channel_size: self.cve_list_write_channel_size,
            parse_concurrency_source: self.cve_list_parse_concurrency_source,
            write_batch_size_source: self.cve_list_write_batch_size_source,
            write_channel_size_source: self.cve_list_write_channel_size_source,
        }
    }

    /// Build the NPM elaboration limits resolved from this configuration.
    pub fn package_elaboration_limits(&self) -> ElaborationLimits {
        self.package_elaboration_limits.clone()
    }
}

struct ResolvedCveListPipelineConfig {
    parse_concurrency: usize,
    write_batch_size: usize,
    write_channel_size: usize,
    parse_concurrency_source: ConfigValueSource,
    write_batch_size_source: ConfigValueSource,
    write_channel_size_source: ConfigValueSource,
}

fn resolve_cve_list_pipeline_config(
    async_worker_threads: Option<usize>,
    async_max_blocking_threads: Option<usize>,
    parse_concurrency: Option<usize>,
    write_batch_size: Option<usize>,
    write_channel_size: Option<usize>,
) -> ResolvedCveListPipelineConfig {
    let (parse_concurrency, parse_concurrency_source) = match parse_concurrency {
        Some(value) => (value, ConfigValueSource::Explicit),
        None => (
            default_cve_list_parse_concurrency(async_worker_threads, async_max_blocking_threads),
            ConfigValueSource::ComputedDefault,
        ),
    };
    let (write_batch_size, write_batch_size_source) = match write_batch_size {
        Some(value) => (value, ConfigValueSource::Explicit),
        None => (
            DEFAULT_CVE_LIST_WRITE_BATCH_SIZE,
            ConfigValueSource::ComputedDefault,
        ),
    };
    let (write_channel_size, write_channel_size_source) = match write_channel_size {
        Some(value) => (value, ConfigValueSource::Explicit),
        None => (
            default_cve_list_write_channel_size(parse_concurrency, write_batch_size),
            ConfigValueSource::ComputedDefault,
        ),
    };

    ResolvedCveListPipelineConfig {
        parse_concurrency,
        write_batch_size,
        write_channel_size,
        parse_concurrency_source,
        write_batch_size_source,
        write_channel_size_source,
    }
}

/// Write a "separator" line of 80 dashes, used at the start and end of the config report.
macro_rules! write_report_separator {
    ($f:ident) => {
        write!($f, "{:-^80}\n", "")
    };
}

/// Write a single line of the config report.
macro_rules! write_report_line {
    // Note that the "48" value is set by-hand based on the length in characters of the longest
    // configuration key we accept. If we introduce longer configuration keys in the future, this will
    // likely need to be bumped.
    ($f:ident, $key:literal, $value:expr) => {
        write!($f, "{:>48}: {}\n", $key, $value)
    };
}

impl Display for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write_report_separator!(f)?;

        // Make sure to report the source configuration file we're using, if we're using something
        // other than the default. This may help people catch mistakes during debugging, for
        // example if they meant to use the default and forgot. Always good to say where config
        // values are coming from when it's unexpected.
        if self.config_file_path != DEFAULT_CONFIG_FILE {
            write_report_line!(f, "Using configuration file", self.config_file_path)?;
        }

        write_report_line!(f, "server-address", &self.server_address)?;
        write_report_line!(f, "database-connection", &self.database_connection_source)?;

        if self.health_diagnostics_token.is_some() {
            write_report_line!(
                f,
                "health-diagnostics-token",
                &"<redacted file-backed secret>"
            )?;
        }

        if let Some(openapi_dest_path) = &self.openapi_dest_path {
            write_report_line!(f, "openapi-dest-path", openapi_dest_path)?;
        }

        if let Some(kev_url) = &self.kev_url {
            write_report_line!(f, "kev-url", kev_url)?;
        }

        if let Some(kev_refresh_interval) = &self.kev_refresh_interval {
            write_report_line!(f, "kev-refresh-interval", kev_refresh_interval)?;
        }

        if let Some(max_bytes) = self.kev_response_body_max_bytes {
            write_report_line!(f, "kev-response-body-max-bytes", &max_bytes)?;
        }

        if let Some(max_bytes) = self.http_request_body_max_bytes {
            write_report_line!(f, "http-request-body-max-bytes", &max_bytes)?;
        }

        if let Some(behavior) = self.http_early_disconnect_behavior {
            write_report_line!(f, "http-early-disconnect-behavior", &behavior)?;
        }

        if let Some(max_connections) = self.database_max_connections {
            write_report_line!(f, "database-max-connections", &max_connections)?;
        }

        if let Some(min_connections) = self.database_min_connections {
            write_report_line!(f, "database-min-connections", &min_connections)?;
        }

        if let Some(connect_timeout) = self.database_connect_timeout {
            write_report_line!(f, "database-connect-timeout", &connect_timeout)?;
        }

        if let Some(idle_timeout) = self.database_idle_timeout {
            write_report_line!(f, "database-idle-timeout", &idle_timeout)?;
        }

        if let Some(acquire_timeout) = self.database_acquire_timeout {
            write_report_line!(f, "database-acquire-timeout", &acquire_timeout)?;
        }

        if let Some(max_lifetime) = self.database_max_lifetime {
            write_report_line!(f, "database-max-lifetime", &max_lifetime)?;
        }

        if let Some(async_worker_threads) = self.async_worker_threads {
            write_report_line!(f, "async-worker-threads", &async_worker_threads)?;
        }

        if let Some(async_worker_thread_stack_size) = self.async_worker_thread_stack_size {
            write_report_line!(
                f,
                "async-worker-thread-stack-size",
                &async_worker_thread_stack_size
            )?;
        }

        if let Some(async_max_blocking_threads) = self.async_max_blocking_threads {
            write_report_line!(f, "async-max-blocking-threads", &async_max_blocking_threads)?;
        }

        if let Some(async_blocking_thread_keep_alive) = self.async_blocking_thread_keep_alive {
            write_report_line!(
                f,
                "async-blocking-thread-keep-alive",
                &async_blocking_thread_keep_alive
            )?;
        }

        if let Some(async_global_queue_interval) = self.async_global_queue_interval {
            write_report_line!(
                f,
                "async-global-queue-interval",
                &async_global_queue_interval
            )?;
        }

        if let Some(async_event_interval) = self.async_event_interval {
            write_report_line!(f, "async-event-interval", &async_event_interval)?;
        }

        if self.cve_list_repository_url.as_str() != DEFAULT_CVE_LIST_REPOSITORY_URL {
            write_report_line!(f, "cve-list-repository-url", &self.cve_list_repository_url)?;
        }

        if self.cve_list_repository_ref.as_str() != DEFAULT_CVE_LIST_REPOSITORY_REF {
            write_report_line!(f, "cve-list-repository-ref", &self.cve_list_repository_ref)?;
        }

        if self.cve_list_sync_interval != DEFAULT_CVE_LIST_SYNC_INTERVAL {
            write_report_line!(f, "cve-list-sync-interval", &self.cve_list_sync_interval)?;
        }

        if self.cve_list_sync_timeout != DEFAULT_CVE_LIST_SYNC_TIMEOUT {
            write_report_line!(f, "cve-list-sync-timeout", &self.cve_list_sync_timeout)?;
        }

        if self.cve_list_first_sync_timeout != DEFAULT_CVE_LIST_FIRST_SYNC_TIMEOUT {
            write_report_line!(
                f,
                "cve-list-first-sync-timeout",
                &self.cve_list_first_sync_timeout
            )?;
        }

        write_report_line!(f, "cve-list-checkout-path", &self.cve_list_checkout_path)?;
        if self.cve_list_parse_concurrency
            != default_cve_list_parse_concurrency(
                self.async_worker_threads,
                self.async_max_blocking_threads,
            )
        {
            write_report_line!(
                f,
                "cve-list-parse-concurrency",
                &self.cve_list_parse_concurrency
            )?;
        }

        if self.cve_list_write_batch_size != DEFAULT_CVE_LIST_WRITE_BATCH_SIZE {
            write_report_line!(
                f,
                "cve-list-write-batch-size",
                &self.cve_list_write_batch_size
            )?;
        }

        if self.cve_record_max_bytes != DEFAULT_CVE_RECORD_MAX_BYTES {
            write_report_line!(f, "cve-record-max-bytes", &self.cve_record_max_bytes)?;
        }

        if self.cve_list_write_channel_size
            != default_cve_list_write_channel_size(
                self.cve_list_parse_concurrency,
                self.cve_list_write_batch_size,
            )
        {
            write_report_line!(
                f,
                "cve-list-write-channel-size",
                &self.cve_list_write_channel_size
            )?;
        }

        write_report_line!(f, "npm-registry-url", &self.npm_registry_url)?;
        write_report_line!(
            f,
            "package-elaboration-worker-concurrency",
            &self.package_elaboration_limits.worker_concurrency
        )?;
        write_report_line!(
            f,
            "package-elaboration-work-queue-capacity",
            &self.package_elaboration_limits.work_queue_capacity
        )?;
        write_report_line!(
            f,
            "package-elaboration-max-packument-bytes",
            &self.package_elaboration_max_packument_bytes
        )?;
        write_report_line!(
            f,
            "package-elaboration-request-timeout",
            &self.package_elaboration_limits.request_timeout.as_millis()
        )?;
        write_report_line!(
            f,
            "package-elaboration-total-run-timeout",
            &self
                .package_elaboration_limits
                .total_run_timeout
                .as_millis()
        )?;
        write_report_line!(
            f,
            "package-elaboration-max-packages",
            &self.package_elaboration_limits.max_packages
        )?;
        write_report_line!(
            f,
            "package-elaboration-max-edges",
            &self.package_elaboration_limits.max_edges
        )?;
        write_report_line!(
            f,
            "package-elaboration-max-queued-work",
            &self.package_elaboration_limits.max_queued_work
        )?;
        write_report_line!(
            f,
            "package-elaboration-max-derivations",
            &self.package_elaboration_limits.max_derivations
        )?;
        write_report_line!(
            f,
            "package-elaboration-max-concurrent-runs",
            &self.package_elaboration_max_concurrent_runs
        )?;

        write_report_separator!(f)?;

        Ok(())
    }
}

fn default_cve_list_repository_url() -> Url {
    Url::parse(DEFAULT_CVE_LIST_REPOSITORY_URL).expect("default CVE List repository URL is valid")
}

fn default_npm_registry_url() -> Url {
    Url::parse(DEFAULT_NPM_REGISTRY_URL).expect("default NPM registry URL is valid")
}

struct PackageElaborationLimitSettings {
    worker_concurrency: Option<usize>,
    work_queue_capacity: Option<usize>,
    request_timeout: u64,
    total_run_timeout: u64,
    max_packages: usize,
    max_edges: usize,
    max_queued_work: usize,
    max_derivations: usize,
}

fn resolve_package_elaboration_limits(
    async_worker_threads: Option<usize>,
    settings: PackageElaborationLimitSettings,
) -> ElaborationLimits {
    let worker_concurrency = settings.worker_concurrency.unwrap_or_else(|| {
        async_worker_threads
            .unwrap_or_else(available_parallelism)
            .saturating_mul(4)
            .clamp(4, 32)
    });
    ElaborationLimits {
        worker_concurrency,
        work_queue_capacity: settings
            .work_queue_capacity
            .unwrap_or_else(|| worker_concurrency.saturating_mul(4)),
        request_timeout: Duration::from_millis(settings.request_timeout),
        total_run_timeout: Duration::from_millis(settings.total_run_timeout),
        max_packages: settings.max_packages,
        max_edges: settings.max_edges,
        max_queued_work: settings.max_queued_work,
        max_derivations: settings.max_derivations,
    }
}

/// Parse the CVE List repository URL, rejecting embedded authentication.
fn parse_cve_list_repository_url(
    parsed: &spookey::ParseResult,
    errors: &mut Vec<ConfigError>,
) -> Option<Url> {
    let url = parse_value::<Url>(parsed, "cve-list-repository-url", errors)?;

    if url.username().is_empty() && url.password().is_none() {
        return Some(url);
    }

    errors.push(ConfigError::StrParse(StrParseError {
        key: "cve-list-repository-url".into(),
        value: redact_url_authentication(&url).as_str().into(),
        err: "repository URLs must not include authentication information".into(),
    }));
    None
}

/// Return a copy of a URL with its authentication components redacted.
pub fn redact_url_authentication(url: &Url) -> Url {
    let mut redacted = url.clone();

    if !redacted.username().is_empty() {
        redacted
            .set_username("<redacted>")
            .expect("URLs with usernames support replacing their username");
    }
    if redacted.password().is_some() {
        redacted
            .set_password(Some("<redacted>"))
            .expect("URLs with passwords support replacing their password");
    }

    redacted
}

fn default_cve_list_repository_ref() -> GitRef {
    GitRef::parse(DEFAULT_CVE_LIST_REPOSITORY_REF)
        .expect("default CVE List repository ref is valid")
}

fn default_cve_list_parse_concurrency(
    async_worker_threads: Option<usize>,
    async_max_blocking_threads: Option<usize>,
) -> usize {
    let worker_threads = async_worker_threads.unwrap_or_else(available_parallelism);
    let blocking_threads = async_max_blocking_threads.unwrap_or(DEFAULT_ASYNC_MAX_BLOCKING_THREADS);
    let cpu_scaled = worker_threads
        .saturating_mul(2)
        .max(MIN_COMPUTED_CVE_LIST_PARSE_CONCURRENCY);

    cpu_scaled
        .min(DEFAULT_CVE_LIST_PARSE_CONCURRENCY_CAP)
        .min(blocking_threads)
        .max(1)
}

fn default_cve_list_write_channel_size(parse_concurrency: usize, write_batch_size: usize) -> usize {
    write_batch_size
        .saturating_mul(2)
        .max(parse_concurrency.saturating_mul(4))
        .clamp(1, DEFAULT_CVE_LIST_WRITE_CHANNEL_SIZE_CAP)
}

fn available_parallelism() -> usize {
    std::thread::available_parallelism().map_or(1, usize::from)
}

/// Parse a value from the config map, returning `None` if the value is unset.
///
/// This also records any parsing failures as warnings.
fn parse_value<T: std::str::FromStr>(
    results: &spookey::ParseResult,
    key: &str,
    errors: &mut Vec<ConfigError>,
) -> Option<T>
where
    <T as FromStr>::Err: std::error::Error,
{
    match (
        results.required_keys.get(key),
        results.optional_keys.get(key),
    ) {
        // Unknown key.
        (None, None) => None,
        // Known key, but not set in the parsed document.
        (None, Some(None)) => None,
        // Known key, set in the parsed document.
        (None, Some(Some(value))) | (Some(value), None) => match value.parse() {
            Ok(value) => Some(value),
            Err(err) => {
                errors.push(ConfigError::StrParse(StrParseError {
                    key: key.to_owned().into_boxed_str(),
                    value: value.clone().into_boxed_str(),
                    // Make sure we package up the full error chain, not just the top-level error.
                    err: err
                        .sources_iter()
                        .map(ToString::to_string)
                        .join(": ")
                        .into_boxed_str(),
                }));
                // We treat parse errors as not setting the key; though this is meaningless since
                // parse errors are treated as fatal anyway, so the program won't continue past
                // construction of `Config` if one occurs.
                None
            }
        },
        // Known key, somehow found in both "required" and "optional" maps.
        //
        // If for some reason we trigger this, it indicates a bug in `spookey`.
        (Some(_), Some(_)) => unreachable!(
            "spookey doesn't permit a single key to be in `required_keys` and `optional_keys`"
        ),
    }
}

fn parse_positive_usize_option(
    parsed: &spookey::ParseResult,
    key: &'static str,
    errors: &mut Vec<ConfigError>,
) -> Option<usize> {
    let value = parse_value::<usize>(parsed, key, errors)?;

    if value == 0 {
        errors.push(ConfigError::StrParse(StrParseError {
            key: key.to_owned().into_boxed_str(),
            value: value.to_string().into_boxed_str(),
            err: "must be greater than 0".into(),
        }));

        None
    } else {
        Some(value)
    }
}

fn parse_positive_usize(
    parsed: &spookey::ParseResult,
    key: &'static str,
    default: usize,
    errors: &mut Vec<ConfigError>,
) -> usize {
    let Some(value) = parse_positive_usize_option(parsed, key, errors) else {
        return default;
    };

    value
}

fn parse_positive_u64_option(
    parsed: &spookey::ParseResult,
    key: &'static str,
    errors: &mut Vec<ConfigError>,
) -> Option<u64> {
    let value = parse_value::<u64>(parsed, key, errors)?;

    if value == 0 {
        errors.push(ConfigError::StrParse(StrParseError {
            key: key.to_owned().into_boxed_str(),
            value: value.to_string().into_boxed_str(),
            err: "must be greater than 0".into(),
        }));

        None
    } else {
        Some(value)
    }
}

fn parse_positive_u64(
    parsed: &spookey::ParseResult,
    key: &'static str,
    default: u64,
    errors: &mut Vec<ConfigError>,
) -> u64 {
    parse_positive_u64_option(parsed, key, errors).unwrap_or(default)
}

fn parse_kev_refresh_interval(
    parsed: &spookey::ParseResult,
    key: &'static str,
    errors: &mut Vec<ConfigError>,
) -> Option<u64> {
    // Jiff accepts at most 631,107,417,600,000 milliseconds in a Span.
    const MAX_JIFF_SPAN_MILLISECONDS: u64 = 631_107_417_600_000;

    let value = parse_positive_u64_option(parsed, key, errors)?;
    if value > MAX_JIFF_SPAN_MILLISECONDS {
        errors.push(ConfigError::StrParse(StrParseError {
            key: key.to_owned().into_boxed_str(),
            value: value.to_string().into_boxed_str(),
            err: format!("must not exceed {MAX_JIFF_SPAN_MILLISECONDS}").into_boxed_str(),
        }));
        None
    } else {
        Some(value)
    }
}

/// Parse the mutually-exclusive database connection source fields.
fn parse_database_connection_source(
    database_connection: Option<SecretString>,
    database_connection_file: Option<Utf8PathBuf>,
    errors: &mut Vec<ConfigError>,
) -> SecretSource {
    match (database_connection, database_connection_file) {
        (Some(value), None) => SecretSource::inline(value),
        (None, Some(path)) => SecretSource::file(path),
        (Some(_), Some(_)) => {
            errors.push(ConfigError::MutuallyExclusive {
                keys: Box::new(["database-connection", "database-connection-file"]),
            });
            SecretSource::inline(String::new().into())
        }
        (None, None) => {
            errors.push(ConfigError::MissingOneOf {
                keys: Box::new(["database-connection", "database-connection-file"]),
            });
            SecretSource::inline(String::new().into())
        }
    }
}

/// Resolve the database connection source, preserving file path context for startup errors.
fn resolve_database_connection_source(
    source: SecretSource,
) -> Result<(SecretSourceKind, SecretString), ConfigLoadError> {
    source
        .resolve()
        .map(crate::secret::ResolvedSecret::into_parts)
        .map_err(|err| ConfigLoadError::FailedToReadSecretFile(err.path, err.error))
}

/// Resolve an optional file-backed secret, preserving path context for errors.
fn resolve_optional_secret_file(
    path: Option<Utf8PathBuf>,
) -> Result<Option<SecretString>, ConfigLoadError> {
    path.map(|path| {
        SecretSource::file(path)
            .resolve()
            .map(crate::secret::ResolvedSecret::into_parts)
            .map(|(_, secret)| secret)
            .map_err(|err| ConfigLoadError::FailedToReadSecretFile(err.path, err.error))
    })
    .transpose()
}

/// Check collected warnings and field parsing errors, bundling them in a `ConfigLoadError` to report.
fn check_errors(
    path: &Utf8Path,
    parsed_warnings: Vec<spookey::Warning>,
    value_parsing_errors: Vec<ConfigError>,
) -> Result<(), ConfigLoadError> {
    if parsed_warnings.is_empty() && value_parsing_errors.is_empty() {
        return Ok(());
    }

    let mut errors = vec![];

    for warning in parsed_warnings {
        errors.push(ConfigError::Warning(warning));
    }

    for error in value_parsing_errors {
        errors.push(error);
    }

    Err(ConfigLoadError::FailedToParseConfigFileFields(
        path.to_owned(),
        ConfigErrors(errors),
    ))
}

/// How to handle early disconnects from clients.
#[derive(Clone, Copy, Debug)]
pub enum EarlyDisconnectBehavior {
    /// Cancel the handler task on a disconnect.
    Cancel,
    /// Run the handler to completion even after the client disconnects.
    Continue,
}

impl FromStr for EarlyDisconnectBehavior {
    type Err = EarlyDisconnectParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "cancel" => Ok(Self::Cancel),
            "continue" => Ok(Self::Continue),
            s => Err(EarlyDisconnectParseError(s.to_owned().into_boxed_str())),
        }
    }
}

impl std::fmt::Display for EarlyDisconnectBehavior {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancel => write!(f, "cancel"),
            Self::Continue => write!(f, "continue"),
        }
    }
}

impl From<EarlyDisconnectBehavior> for HandlerTaskMode {
    fn from(value: EarlyDisconnectBehavior) -> Self {
        match value {
            EarlyDisconnectBehavior::Cancel => Self::CancelOnDisconnect,
            EarlyDisconnectBehavior::Continue => Self::Detached,
        }
    }
}

/// An error arising from attempting to parse the `EarlyDisconnect` type from a string.
///
/// Just wraps the invalid value and explains what was expected.
#[derive(Debug)]
pub struct EarlyDisconnectParseError(Box<str>);

impl Display for EarlyDisconnectParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "invalid early disconnect behavior (must be 'cancel' or 'continue'): {}",
            self.0
        )
    }
}

impl std::error::Error for EarlyDisconnectParseError {}

/// Failure to parse a single configuration file key/value.
///
/// This is our general error type for failures of `value.parse()`, attempting to parse a string
/// into an arbitrary type in the `Config` struct. Rather than being generic and wrapping an
/// arbitrary `E: Error` type from the `value` type's `FromStr` impl, we stringify the error.
/// This means we can very easily store a bunch of these `StrParseError`s together to gather up
/// parse errors to be reported together, rather than needing a more awkward mechanism to handle
/// the disparate `E: Error` types in a single `Vec`.
pub struct StrParseError {
    /// The key of the value that failed to parse.
    pub key: Box<str>,
    /// The value that failed to parse.
    pub value: Box<str>,
    /// The error message from the parser (converted to a string).
    pub err: Box<str>,
}

impl Debug for StrParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "failed to parse key '{}' with value '{}': {}",
            self.key, self.value, self.err
        )
    }
}

impl Display for StrParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "failed to parse key '{}' with value '{}': {}",
            self.key, self.value, self.err
        )
    }
}

/// Bundles up `spookey` warnings and field parse errors.
///
/// `spookey` provides a bunch of warnings for invalid configuration lines, but leaves it up to
/// the caller whether to treat them as errors and how to handle them.
///
/// Additionally, `spookey` doesn't attempt to parse values into structured types, it leaves
/// them as strings.
///
/// This `ConfigError` type represents both `spookey` errors (indicating structurally-invalid
/// config lines) and our own value-parsing errors.
#[derive(Debug)]
pub enum ConfigError {
    Warning(spookey::Warning),
    StrParse(StrParseError),
    MissingOneOf { keys: Box<[&'static str; 2]> },
    MutuallyExclusive { keys: Box<[&'static str; 2]> },
}

impl Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Warning(w) => write!(f, "{w}"),
            Self::StrParse(e) => write!(f, "{e}"),
            Self::MissingOneOf { keys } => write!(
                f,
                "exactly one of '{}' or '{}' must be configured",
                keys[0], keys[1]
            ),
            Self::MutuallyExclusive { keys } => {
                write!(f, "'{}' and '{}' are mutually exclusive", keys[0], keys[1])
            }
        }
    }
}

/// Bundles up a collection of [`ConfigError`]s.
#[derive(Debug)]
pub struct ConfigErrors(Vec<ConfigError>);

impl Display for ConfigErrors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = self.0.iter().map(ToString::to_string).join(", ");
        write!(f, "config errors: {msg}")
    }
}

impl std::error::Error for ConfigErrors {}

#[cfg(test)]
mod tests {
    use crate::secret::SecretFileError;
    #[cfg(unix)]
    use crate::test_util::TestFilePermissions;
    use crate::test_util::restrict_secret_file_permissions;
    #[cfg(unix)]
    use crate::test_util::set_file_permissions;
    use secrecy::ExposeSecret as _;
    use std::{
        fs,
        sync::atomic::{AtomicUsize, Ordering},
    };

    #[cfg(windows)]
    use std::process::Command;

    use super::*;

    static TEST_FILE_COUNTER: AtomicUsize = AtomicUsize::new(0);

    struct TempConfigFile {
        path: Utf8PathBuf,
    }

    impl TempConfigFile {
        fn new(contents: &str) -> Self {
            let id = TEST_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "nv-server-config-test-{}-{id}.spookey",
                std::process::id()
            ));
            fs::write(&path, contents).expect("failed to write test config file");

            Self {
                path: Utf8PathBuf::from_path_buf(path)
                    .expect("test temp path should be valid UTF-8"),
            }
        }

        fn path(&self) -> &Utf8Path {
            &self.path
        }
    }

    impl Drop for TempConfigFile {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.path);
        }
    }

    #[cfg(windows)]
    struct GrantReadAclGuard<'a> {
        path: &'a Utf8Path,
    }

    #[cfg(windows)]
    impl<'a> GrantReadAclGuard<'a> {
        fn new(path: &'a Utf8Path) -> Self {
            // Grants access to "everyone". `*S-1-1-0` is the SID for "Everyone" and `:R` means
            // we're setting a read permission, so this makes our secure file readable to everyone.
            let status = Command::new("icacls")
                .arg(path.as_std_path())
                .args(["/grant", "*S-1-1-0:R"])
                .status()
                .expect("failed to grant test file read access");
            assert!(status.success(), "failed to grant test file read access");

            Self { path }
        }
    }

    #[cfg(windows)]
    impl Drop for GrantReadAclGuard<'_> {
        fn drop(&mut self) {
            let _ = Command::new("icacls")
                .arg(self.path.as_std_path())
                .args(["/remove:g", "*S-1-1-0"])
                .status();
        }
    }

    fn redact_database_conn_password(conn: &str) -> String {
        let Some((scheme, rest)) = conn.split_once("://") else {
            return conn.to_owned();
        };

        let Some((userinfo, host_and_path)) = rest.split_once('@') else {
            return conn.to_owned();
        };

        let Some((user, _password)) = userinfo.rsplit_once(':') else {
            return conn.to_owned();
        };

        format!("{scheme}://{user}:<redacted>@{host_and_path}")
    }

    fn valid_required_config() -> String {
        [
            // The port here is irrelevant; we're not making real connections in these tests.
            "server-address = 127.0.0.1:0",
            // We don't actually use sqlite; but we're not making real DB connections in these
            // tests, only validating that we can parse the config field correctly.
            "database-connection = sqlite::memory:",
            "cve-list-checkout-path = /tmp/night-vision-test-cvelistV5",
        ]
        .join("\n")
    }

    struct ConfigFieldParseCase {
        name: &'static str,
        server_address: &'static str,
        database_connection: &'static str,
        extra_config: &'static str,
        assert: fn(&Config, &Utf8Path),
    }

    impl ConfigFieldParseCase {
        fn new(
            name: &'static str,
            extra_config: &'static str,
            assert: fn(&Config, &Utf8Path),
        ) -> Self {
            Self {
                name,
                server_address: "127.0.0.1:0",
                database_connection: "sqlite::memory:",
                extra_config,
                assert,
            }
        }

        fn with_server_address(mut self, server_address: &'static str) -> Self {
            self.server_address = server_address;
            self
        }

        fn with_database_connection(mut self, database_connection: &'static str) -> Self {
            self.database_connection = database_connection;
            self
        }

        fn contents(&self) -> String {
            let mut lines = vec![
                format!("server-address = {}", self.server_address),
                format!("database-connection = {}", self.database_connection),
            ];

            if !self.extra_config.contains("cve-list-checkout-path") {
                lines.push("cve-list-checkout-path = /tmp/night-vision-test-cvelistV5".to_owned());
            }

            lines.push(self.extra_config.to_owned());
            lines.join("\n")
        }
    }

    #[test]
    fn parses_every_config_field_from_spookey_config() {
        let cases = [
            ConfigFieldParseCase::new("config_file_path", "", |config, path| {
                assert_eq!(config.config_file_path, path);
            }),
            ConfigFieldParseCase::new("server_address", "", |config, _| {
                assert_eq!(config.server_address, "0.0.0.0:9000");
            })
            .with_server_address("0.0.0.0:9000"),
            ConfigFieldParseCase::new(
                "kev_refresh_interval",
                "kev-refresh-interval = 900000",
                |config, _| {
                    assert_eq!(config.kev_refresh_interval, Some(900_000));
                },
            ),
            ConfigFieldParseCase::new(
                "kev_response_body_max_bytes",
                "kev-response-body-max-bytes = 2048",
                |config, _| {
                    assert_eq!(config.kev_response_body_max_bytes, Some(2048));
                },
            ),
            ConfigFieldParseCase::new(
                "http_request_body_max_bytes",
                "http-request-body-max-bytes = 2048",
                |config, _| {
                    assert_eq!(config.http_request_body_max_bytes, Some(2048));
                },
            ),
            ConfigFieldParseCase::new(
                "http_early_disconnect_behavior",
                "http-early-disconnect-behavior = cancel",
                |config, _| {
                    assert!(matches!(
                        config.http_early_disconnect_behavior,
                        Some(EarlyDisconnectBehavior::Cancel)
                    ));
                },
            ),
            ConfigFieldParseCase::new("database_connection", "", |config, _| {
                assert!(matches!(
                    config.database_connection_source,
                    SecretSourceKind::Inline
                ));
                assert_eq!(
                    config.database_connection().expose_secret(),
                    "postgres://localhost:5432/nv"
                );
            })
            .with_database_connection("postgres://localhost:5432/nv"),
            ConfigFieldParseCase::new(
                "database_max_connections",
                "database-max-connections = 25",
                |config, _| {
                    assert_eq!(config.database_max_connections, Some(25));
                },
            ),
            ConfigFieldParseCase::new(
                "database_min_connections",
                "database-min-connections = 5",
                |config, _| {
                    assert_eq!(config.database_min_connections, Some(5));
                },
            ),
            ConfigFieldParseCase::new(
                "database_connect_timeout",
                "database-connect-timeout = 1000",
                |config, _| {
                    assert_eq!(config.database_connect_timeout, Some(1000));
                },
            ),
            ConfigFieldParseCase::new(
                "database_idle_timeout",
                "database-idle-timeout = 2000",
                |config, _| {
                    assert_eq!(config.database_idle_timeout, Some(2000));
                },
            ),
            ConfigFieldParseCase::new(
                "database_acquire_timeout",
                "database-acquire-timeout = 3000",
                |config, _| {
                    assert_eq!(config.database_acquire_timeout, Some(3000));
                },
            ),
            ConfigFieldParseCase::new(
                "database_max_lifetime",
                "database-max-lifetime = 4000",
                |config, _| {
                    assert_eq!(config.database_max_lifetime, Some(4000));
                },
            ),
            ConfigFieldParseCase::new(
                "async_worker_threads",
                "async-worker-threads = 8",
                |config, _| {
                    assert_eq!(config.async_worker_threads, Some(8));
                },
            ),
            ConfigFieldParseCase::new(
                "async_worker_thread_stack_size",
                "async-worker-thread-stack-size = 2097152",
                |config, _| {
                    assert_eq!(config.async_worker_thread_stack_size, Some(2_097_152));
                },
            ),
            ConfigFieldParseCase::new(
                "async_max_blocking_threads",
                "async-max-blocking-threads = 64",
                |config, _| {
                    assert_eq!(config.async_max_blocking_threads, Some(64));
                },
            ),
            ConfigFieldParseCase::new(
                "async_blocking_thread_keep_alive",
                "async-blocking-thread-keep-alive = 5000",
                |config, _| {
                    assert_eq!(config.async_blocking_thread_keep_alive, Some(5000));
                },
            ),
            ConfigFieldParseCase::new(
                "async_global_queue_interval",
                "async-global-queue-interval = 31",
                |config, _| {
                    assert_eq!(config.async_global_queue_interval, Some(31));
                },
            ),
            ConfigFieldParseCase::new(
                "async_event_interval",
                "async-event-interval = 61",
                |config, _| {
                    assert_eq!(config.async_event_interval, Some(61));
                },
            ),
            ConfigFieldParseCase::new(
                "cve_list_repository_url",
                "cve-list-repository-url = https://github.com/CVEProject/cvelistV5.git",
                |config, _| {
                    assert_eq!(
                        config.cve_list_repository_url.as_str(),
                        "https://github.com/CVEProject/cvelistV5.git"
                    );
                },
            ),
            ConfigFieldParseCase::new(
                "cve_list_repository_ref",
                "cve-list-repository-ref = main",
                |config, _| {
                    assert_eq!(config.cve_list_repository_ref.as_str(), "main");
                },
            ),
            ConfigFieldParseCase::new(
                "cve_list_sync_interval",
                "cve-list-sync-interval = 900000",
                |config, _| {
                    assert_eq!(config.cve_list_sync_interval, 900_000);
                },
            ),
            ConfigFieldParseCase::new(
                "cve_list_sync_timeout",
                "cve-list-sync-timeout = 1800000",
                |config, _| {
                    assert_eq!(config.cve_list_sync_timeout, 1_800_000);
                },
            ),
            ConfigFieldParseCase::new(
                "cve_list_first_sync_timeout",
                "cve-list-first-sync-timeout = 7200000",
                |config, _| {
                    assert_eq!(config.cve_list_first_sync_timeout, 7_200_000);
                },
            ),
            ConfigFieldParseCase::new(
                "cve_list_checkout_path",
                "cve-list-checkout-path = /var/cache/night-vision/cvelistV5",
                |config, _| {
                    assert_eq!(
                        config.cve_list_checkout_path,
                        Utf8Path::new("/var/cache/night-vision/cvelistV5")
                    );
                },
            ),
            ConfigFieldParseCase::new(
                "cve_list_parse_concurrency",
                "cve-list-parse-concurrency = 16",
                |config, _| {
                    assert_eq!(config.cve_list_parse_concurrency, 16);
                    assert_eq!(
                        config.cve_list_parse_concurrency_source,
                        ConfigValueSource::Explicit
                    );
                },
            ),
            ConfigFieldParseCase::new(
                "cve_list_write_batch_size",
                "cve-list-write-batch-size = 250",
                |config, _| {
                    assert_eq!(config.cve_list_write_batch_size, 250);
                    assert_eq!(
                        config.cve_list_write_batch_size_source,
                        ConfigValueSource::Explicit
                    );
                },
            ),
            ConfigFieldParseCase::new(
                "cve_record_max_bytes",
                "cve-record-max-bytes = 2097152",
                |config, _| {
                    assert_eq!(config.cve_record_max_bytes, 2_097_152);
                },
            ),
            ConfigFieldParseCase::new(
                "cve_list_write_channel_size",
                "cve-list-write-channel-size = 1024",
                |config, _| {
                    assert_eq!(config.cve_list_write_channel_size, 1024);
                    assert_eq!(
                        config.cve_list_write_channel_size_source,
                        ConfigValueSource::Explicit
                    );
                },
            ),
            ConfigFieldParseCase::new(
                "package_elaboration_max_concurrent_runs",
                "package-elaboration-max-concurrent-runs = 3",
                |config, _| {
                    assert_eq!(config.package_elaboration_max_concurrent_runs, 3);
                },
            ),
        ];

        for case in cases {
            let file = TempConfigFile::new(&case.contents());
            let config = Config::parse(file.path())
                .unwrap_or_else(|error| panic!("{} should parse: {error}", case.name));

            (case.assert)(&config, file.path());
        }
    }

    #[test]
    fn parses_required_and_optional_values_from_spookey_config() {
        let file = TempConfigFile::new(&format!(
            "{}\n{}\n{}\n{}",
            valid_required_config(),
            "http-request-body-max-bytes = 2048",
            "http-early-disconnect-behavior = continue",
            "async-event-interval = 17",
        ));

        let config = Config::parse(file.path()).expect("config should parse");

        assert_eq!(config.server_address, "127.0.0.1:0");
        assert!(matches!(
            config.database_connection_source,
            SecretSourceKind::Inline
        ));
        assert_eq!(
            config.database_connection().expose_secret(),
            "sqlite::memory:"
        );
        assert_eq!(config.http_request_body_max_bytes, Some(2048));
        assert!(matches!(
            config.http_early_disconnect_behavior,
            Some(EarlyDisconnectBehavior::Continue)
        ));
        assert_eq!(config.async_event_interval, Some(17));
        assert_eq!(
            config.cve_list_repository_url.as_str(),
            DEFAULT_CVE_LIST_REPOSITORY_URL
        );
        assert_eq!(
            config.cve_list_repository_ref.as_str(),
            DEFAULT_CVE_LIST_REPOSITORY_REF
        );
        assert_eq!(
            config.cve_list_sync_interval,
            DEFAULT_CVE_LIST_SYNC_INTERVAL
        );
        assert_eq!(config.cve_list_sync_timeout, DEFAULT_CVE_LIST_SYNC_TIMEOUT);
        assert_eq!(
            config.cve_list_first_sync_timeout,
            DEFAULT_CVE_LIST_FIRST_SYNC_TIMEOUT
        );
        assert_eq!(
            config.cve_list_checkout_path,
            Utf8Path::new("/tmp/night-vision-test-cvelistV5")
        );
        assert_eq!(
            config.cve_list_write_batch_size,
            DEFAULT_CVE_LIST_WRITE_BATCH_SIZE
        );
        assert_eq!(config.cve_record_max_bytes, DEFAULT_CVE_RECORD_MAX_BYTES);
        assert_eq!(config.npm_registry_url.as_str(), DEFAULT_NPM_REGISTRY_URL);
        assert_eq!(
            config.hipcheck_working_dir,
            Utf8Path::new(DEFAULT_HIPCHECK_CACHE_DIR)
        );
        assert_eq!(
            config.hipcheck_cache_dir,
            Utf8Path::new(DEFAULT_HIPCHECK_CACHE_DIR)
        );
        assert_eq!(
            config.hipcheck_runner_config().environment,
            BTreeMap::from([
                (OsString::from("PATH"), OsString::from(HIPCHECK_SYSTEM_PATH)),
                (
                    OsString::from("TMPDIR"),
                    OsString::from(format!("{DEFAULT_HIPCHECK_CACHE_DIR}/tmp")),
                ),
            ])
        );
        assert_eq!(
            config.package_elaboration_max_packument_bytes,
            64 * 1024 * 1024
        );
        assert_eq!(
            config.package_elaboration_limits.request_timeout,
            Duration::from_millis(DEFAULT_PACKAGE_ELABORATION_REQUEST_TIMEOUT)
        );
        assert_eq!(
            config.package_elaboration_limits.total_run_timeout,
            Duration::from_millis(DEFAULT_PACKAGE_ELABORATION_TOTAL_RUN_TIMEOUT)
        );
        assert_eq!(config.package_elaboration_limits.max_packages, 10_000);
        assert_eq!(config.package_elaboration_limits.max_edges, 1_000_000);
        assert_eq!(config.package_elaboration_limits.max_queued_work, 100_000);
        assert_eq!(config.package_elaboration_limits.max_derivations, 1_000_000);
        assert_eq!(
            config.package_elaboration_max_concurrent_runs,
            DEFAULT_PACKAGE_ELABORATION_MAX_CONCURRENT_RUNS
        );
        assert_eq!(
            config.cve_list_parse_concurrency,
            default_cve_list_parse_concurrency(None, None)
        );
        assert_eq!(
            config.cve_list_write_channel_size,
            default_cve_list_write_channel_size(
                config.cve_list_parse_concurrency,
                config.cve_list_write_batch_size
            )
        );
        assert_eq!(
            config.cve_list_parse_concurrency_source,
            ConfigValueSource::ComputedDefault
        );
        assert_eq!(
            config.cve_list_write_batch_size_source,
            ConfigValueSource::ComputedDefault
        );
        assert_eq!(
            config.cve_list_write_channel_size_source,
            ConfigValueSource::ComputedDefault
        );
    }

    #[test]
    fn computes_cve_pipeline_defaults_from_runtime_settings() {
        let file = TempConfigFile::new(&format!(
            "{}\n{}\n{}\n{}",
            valid_required_config(),
            "async-worker-threads = 2",
            "async-max-blocking-threads = 3",
            "cve-list-write-batch-size = 10",
        ));

        let config = Config::parse(file.path()).expect("config should parse");

        assert_eq!(config.cve_list_parse_concurrency, 3);
        assert_eq!(config.cve_list_write_batch_size, 10);
        assert_eq!(config.cve_list_write_channel_size, 20);
        assert_eq!(
            config.cve_list_parse_concurrency_source,
            ConfigValueSource::ComputedDefault
        );
        assert_eq!(
            config.cve_list_write_batch_size_source,
            ConfigValueSource::Explicit
        );
        assert_eq!(
            config.cve_list_write_channel_size_source,
            ConfigValueSource::ComputedDefault
        );
    }

    #[test]
    fn parses_database_connection_file_from_spookey_config() {
        let secret_file = TempConfigFile::new("postgres://user:password@localhost:5432/nv\n");
        restrict_secret_file_permissions(secret_file.path());
        let file = TempConfigFile::new(&format!(
            "server-address = 127.0.0.1:0\n\
            cve-list-checkout-path = /tmp/night-vision-test-cvelistV5\n\
            database-connection-file = {}\n",
            secret_file.path()
        ));

        let config = Config::parse(file.path()).expect("config should parse");

        assert_eq!(
            config.database_connection().expose_secret(),
            "postgres://user:password@localhost:5432/nv"
        );
        assert!(matches!(
            config.database_connection_source,
            SecretSourceKind::File
        ));
    }

    #[test]
    fn health_diagnostics_token_file_is_optional_and_redacted() {
        let disabled = TempConfigFile::new(
            "server-address = 127.0.0.1:0\n\
            cve-list-checkout-path = /tmp/night-vision-test-cvelistV5\n\
            database-connection = sqlite::memory:\n",
        );
        let disabled_config = Config::parse(disabled.path()).expect("config should parse");
        assert!(disabled_config.health_diagnostics_token().is_none());

        let secret_file = TempConfigFile::new("operator-token\n");
        restrict_secret_file_permissions(secret_file.path());
        let enabled = TempConfigFile::new(&format!(
            "server-address = 127.0.0.1:0\n\
            cve-list-checkout-path = /tmp/night-vision-test-cvelistV5\n\
            database-connection = sqlite::memory:\n\
            health-diagnostics-token-file = {}\n",
            secret_file.path()
        ));
        let enabled_config = Config::parse(enabled.path()).expect("config should parse");
        assert_eq!(
            enabled_config
                .health_diagnostics_token()
                .expect("token should be configured")
                .expose_secret(),
            "operator-token"
        );
        let report = enabled_config.to_string();
        assert!(report.contains("<redacted file-backed secret>"));
        assert!(!report.contains("operator-token"));
        assert!(!report.contains(secret_file.path().as_str()));
    }

    #[test]
    fn health_diagnostics_token_file_rejects_empty_multiline_and_missing_files() {
        for (contents, expects_multiline) in
            [("\n", false), ("operator-token\nsecond-line\n", true)]
        {
            let secret_file = TempConfigFile::new(contents);
            restrict_secret_file_permissions(secret_file.path());
            let config_file = TempConfigFile::new(&format!(
                "server-address = 127.0.0.1:0\n\
                cve-list-checkout-path = /tmp/night-vision-test-cvelistV5\n\
                database-connection = sqlite::memory:\n\
                health-diagnostics-token-file = {}\n",
                secret_file.path()
            ));

            let error = Config::parse(config_file.path()).expect_err("secret file should fail");
            let error_string = error.to_string();
            let ConfigLoadError::FailedToReadSecretFile(_, secret_error) = error else {
                panic!("expected FailedToReadSecretFile, got {error:?}");
            };
            assert!(if expects_multiline {
                matches!(secret_error, SecretFileError::MultipleLines)
            } else {
                matches!(secret_error, SecretFileError::Empty)
            });
            if !contents.trim().is_empty() {
                assert!(!error_string.contains(contents.trim()));
            }
        }

        let config_file = TempConfigFile::new(
            "server-address = 127.0.0.1:0\n\
            cve-list-checkout-path = /tmp/night-vision-test-cvelistV5\n\
            database-connection = sqlite::memory:\n\
            health-diagnostics-token-file = /tmp/night-vision-missing-diagnostics-token\n",
        );
        let error = Config::parse(config_file.path()).expect_err("missing secret file should fail");
        assert!(matches!(
            error,
            ConfigLoadError::FailedToReadSecretFile(_, SecretFileError::Metadata(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn health_diagnostics_token_file_rejects_insecure_permissions() {
        let secret_file = TempConfigFile::new("operator-token\n");
        set_file_permissions(
            secret_file.path(),
            TestFilePermissions::UnixGroupOrWorldReadable,
        );
        let config_file = TempConfigFile::new(&format!(
            "server-address = 127.0.0.1:0\n\
            cve-list-checkout-path = /tmp/night-vision-test-cvelistV5\n\
            database-connection = sqlite::memory:\n\
            health-diagnostics-token-file = {}\n",
            secret_file.path()
        ));

        let error = Config::parse(config_file.path()).expect_err("secret file should fail");
        assert!(matches!(
            error,
            ConfigLoadError::FailedToReadSecretFile(_, SecretFileError::InsecurePermissions(_))
        ));
    }

    #[test]
    fn config_display_redacts_inline_database_connection() {
        let file = TempConfigFile::new(
            "server-address = 127.0.0.1:0\n\
            cve-list-checkout-path = /tmp/night-vision-test-cvelistV5\n\
            database-connection = postgres://user:password@localhost:5432/nv\n",
        );

        let config = Config::parse(file.path()).expect("config should parse");
        let report = format!("{config}");

        assert!(report.contains("<redacted inline secret>"));
        assert!(!report.contains("password"));
        assert!(!report.contains("postgres://user:password@localhost:5432/nv"));
    }

    #[test]
    fn config_display_redacts_database_connection_file_path_and_contents() {
        let secret_file = TempConfigFile::new("postgres://user:password@localhost:5432/nv\n");
        restrict_secret_file_permissions(secret_file.path());
        let file = TempConfigFile::new(&format!(
            "server-address = 127.0.0.1:0\n\
            cve-list-checkout-path = /tmp/night-vision-test-cvelistV5\n\
            database-connection-file = {}\n",
            secret_file.path()
        ));

        let config = Config::parse(file.path()).expect("config should parse");
        let report = format!("{config}");

        assert!(report.contains("<redacted file-backed secret>"));
        assert!(!report.contains(secret_file.path().as_str()));
        assert!(!report.contains("password"));
        assert!(!report.contains("postgres://user:password@localhost:5432/nv"));
    }

    #[test]
    fn config_display_omits_default_cve_list_settings() {
        let default_parse_concurrency = default_cve_list_parse_concurrency(None, None);
        let default_write_channel_size = default_cve_list_write_channel_size(
            default_parse_concurrency,
            DEFAULT_CVE_LIST_WRITE_BATCH_SIZE,
        );
        let file = TempConfigFile::new(
            &[
                valid_required_config(),
                format!("cve-list-repository-url = {DEFAULT_CVE_LIST_REPOSITORY_URL}"),
                format!("cve-list-repository-ref = {DEFAULT_CVE_LIST_REPOSITORY_REF}"),
                format!("cve-list-sync-interval = {DEFAULT_CVE_LIST_SYNC_INTERVAL}"),
                format!("cve-list-sync-timeout = {DEFAULT_CVE_LIST_SYNC_TIMEOUT}"),
                format!("cve-list-first-sync-timeout = {DEFAULT_CVE_LIST_FIRST_SYNC_TIMEOUT}"),
                format!("cve-list-parse-concurrency = {default_parse_concurrency}"),
                format!("cve-list-write-batch-size = {DEFAULT_CVE_LIST_WRITE_BATCH_SIZE}"),
                format!("cve-record-max-bytes = {DEFAULT_CVE_RECORD_MAX_BYTES}"),
                format!("cve-list-write-channel-size = {default_write_channel_size}"),
            ]
            .join("\n"),
        );

        let config = Config::parse(file.path()).expect("config should parse");
        let report = format!("{config}");

        assert!(report.contains("cve-list-checkout-path"));
        for key in [
            "cve-list-repository-url",
            "cve-list-repository-ref",
            "cve-list-sync-interval",
            "cve-list-sync-timeout",
            "cve-list-first-sync-timeout",
            "cve-list-parse-concurrency",
            "cve-list-write-batch-size",
            "cve-record-max-bytes",
            "cve-list-write-channel-size",
        ] {
            assert!(!report.contains(key), "report unexpectedly contains {key}");
        }
    }

    #[test]
    fn config_display_includes_configured_cve_list_settings() {
        let file = TempConfigFile::new(&format!(
            "{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
            valid_required_config(),
            "cve-list-repository-url = https://example.test/cvelistV5.git",
            "cve-list-repository-ref = stable",
            "cve-list-sync-interval = 900000",
            "cve-list-sync-timeout = 1800000",
            "cve-list-first-sync-timeout = 7200000",
            "cve-list-parse-concurrency = 16",
            "cve-list-write-batch-size = 250",
            "cve-record-max-bytes = 2097152\ncve-list-write-channel-size = 1024",
        ));

        let config = Config::parse(file.path()).expect("config should parse");
        let report = format!("{config}");

        for key in [
            "cve-list-repository-url",
            "cve-list-repository-ref",
            "cve-list-sync-interval",
            "cve-list-sync-timeout",
            "cve-list-first-sync-timeout",
            "cve-list-parse-concurrency",
            "cve-list-write-batch-size",
            "cve-record-max-bytes",
            "cve-list-write-channel-size",
        ] {
            assert!(report.contains(key), "report is missing {key}");
        }
    }

    #[test]
    fn config_rejects_cve_list_repository_url_with_authentication() {
        let file = TempConfigFile::new(
            "server-address = 127.0.0.1:0\n\
            database-connection = sqlite::memory:\n\
            cve-list-checkout-path = /tmp/night-vision-test-cvelistV5\n\
            cve-list-repository-url = https://user:token@example.test/cvelistV5.git\n",
        );

        let error = Config::parse(file.path()).expect_err("config should reject authentication");
        let ConfigLoadError::FailedToParseConfigFileFields(_, ConfigErrors(errors)) = error else {
            panic!("expected a field parsing error");
        };
        let ConfigError::StrParse(parse_error) = &errors[0] else {
            panic!("expected a URL parsing error");
        };

        assert_eq!(
            &*parse_error.value,
            "https://%3Credacted%3E:%3Credacted%3E@example.test/cvelistV5.git"
        );
        assert_eq!(
            &*parse_error.err,
            "repository URLs must not include authentication information"
        );
    }

    #[test]
    fn invalid_typed_field_values_are_returned_as_str_parse_config_errors() {
        let cases = [
            ("http-request-body-max-bytes", "many", "invalid digit"),
            ("kev-response-body-max-bytes", "many", "invalid digit"),
            ("kev-response-body-max-bytes", "0", "must be greater than 0"),
            (
                "http-early-disconnect-behavior",
                "detach",
                "invalid early disconnect behavior",
            ),
            ("database-max-connections", "many", "invalid digit"),
            ("database-min-connections", "few", "invalid digit"),
            ("database-connect-timeout", "soon", "invalid digit"),
            ("database-idle-timeout", "later", "invalid digit"),
            ("database-acquire-timeout", "eventually", "invalid digit"),
            ("database-max-lifetime", "forever", "invalid digit"),
            ("async-worker-threads", "several", "invalid digit"),
            ("async-worker-thread-stack-size", "large", "invalid digit"),
            ("async-max-blocking-threads", "lots", "invalid digit"),
            (
                "async-blocking-thread-keep-alive",
                "briefly",
                "invalid digit",
            ),
            ("async-global-queue-interval", "sometimes", "invalid digit"),
            ("async-event-interval", "often", "invalid digit"),
            ("kev-refresh-interval", "PT1H", "invalid digit"),
            ("kev-refresh-interval", "-1", "invalid digit"),
            ("kev-refresh-interval", "0", "must be greater than 0"),
            (
                "kev-refresh-interval",
                "631107417600001",
                "must not exceed 631107417600000",
            ),
            (
                "cve-list-repository-url",
                "not-a-url",
                "relative URL without a base",
            ),
            ("cve-list-repository-ref", "bad..ref", "invalid Git ref"),
            ("cve-list-sync-interval", "rarely", "invalid digit"),
            ("cve-list-sync-interval", "0", "must be greater than 0"),
            ("cve-list-sync-timeout", "never", "invalid digit"),
            ("cve-list-sync-timeout", "0", "must be greater than 0"),
            ("cve-list-first-sync-timeout", "never", "invalid digit"),
            ("cve-list-first-sync-timeout", "0", "must be greater than 0"),
            ("cve-list-write-batch-size", "0", "must be greater than 0"),
            ("cve-record-max-bytes", "0", "must be greater than 0"),
            ("cve-list-write-channel-size", "0", "must be greater than 0"),
            (
                "package-elaboration-max-concurrent-runs",
                "0",
                "must be greater than 0",
            ),
        ];

        for (key, value, expected_error) in cases {
            let file =
                TempConfigFile::new(&format!("{}\n{} = {}", valid_required_config(), key, value));

            let error =
                Config::parse(file.path()).expect_err(&format!("{key} = {value} should fail"));

            let ConfigLoadError::FailedToParseConfigFileFields(error_path, ConfigErrors(errors)) =
                error
            else {
                panic!("expected FailedToParseConfigFileFields, got {error:?}");
            };

            assert_eq!(error_path, file.path);
            assert_eq!(errors.len(), 1);
            let ConfigError::StrParse(parse_error) = &errors[0] else {
                panic!("expected StrParse config error, got {:?}", errors[0]);
            };
            assert_eq!(&*parse_error.key, key);
            assert_eq!(&*parse_error.value, value);
            assert!(
                parse_error.err.contains(expected_error),
                "expected parse error for {key} to contain {expected_error:?}, got {:?}",
                parse_error.err
            );
        }
    }

    #[test]
    fn missing_config_file_returns_open_config_file_error() {
        let path = Utf8PathBuf::from(format!(
            "/tmp/nv-server-missing-config-test-{}-{}.spookey",
            std::process::id(),
            TEST_FILE_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));

        let error = Config::parse(&path).expect_err("missing config file should fail");

        let ConfigLoadError::FailedToOpenConfigFile(error_path, io_error) = error else {
            panic!("expected FailedToOpenConfigFile, got {error:?}");
        };

        assert_eq!(error_path, path);
        assert_eq!(io_error.kind(), std::io::ErrorKind::NotFound);
    }

    #[test]
    fn missing_database_connection_source_returns_config_field_error() {
        let file = TempConfigFile::new(
            "server-address = 127.0.0.1:0\n\
            cve-list-checkout-path = /tmp/night-vision-test-cvelistV5\n",
        );

        let error =
            Config::parse(file.path()).expect_err("missing database connection should fail");

        let ConfigLoadError::FailedToParseConfigFileFields(error_path, ConfigErrors(errors)) =
            error
        else {
            panic!("expected FailedToParseConfigFileFields, got {error:?}");
        };

        assert_eq!(error_path, file.path);
        assert_eq!(errors.len(), 1);
        let ConfigError::MissingOneOf { keys } = &errors[0] else {
            panic!("expected MissingOneOf config error, got {:?}", errors[0]);
        };
        assert_eq!(keys[0], "database-connection");
        assert_eq!(keys[1], "database-connection-file");
    }

    #[test]
    fn missing_required_key_returns_spookey_parse_error() {
        let file = TempConfigFile::new(
            "database-connection = sqlite::memory:\n\
            cve-list-checkout-path = /tmp/night-vision-test-cvelistV5\n",
        );

        let error = Config::parse(file.path()).expect_err("missing required key should fail");

        let ConfigLoadError::FailedToParseConfigFile(error_path, spookey_error) = error else {
            panic!("expected FailedToParseConfigFile, got {error:?}");
        };

        assert_eq!(error_path, file.path);
        let spookey::Error::MissingRequiredFields(fields) = spookey_error else {
            panic!("expected MissingRequiredFields, got {spookey_error:?}");
        };
        assert_eq!(&*fields, &["server-address"]);
    }

    #[test]
    fn missing_cve_list_checkout_path_returns_spookey_parse_error() {
        let file = TempConfigFile::new(
            "server-address = 127.0.0.1:0\n\
            database-connection = sqlite::memory:\n",
        );

        let error = Config::parse(file.path()).expect_err("missing checkout path should fail");

        let ConfigLoadError::FailedToParseConfigFile(error_path, spookey_error) = error else {
            panic!("expected FailedToParseConfigFile, got {error:?}");
        };

        assert_eq!(error_path, file.path);
        let spookey::Error::MissingRequiredFields(fields) = spookey_error else {
            panic!("expected MissingRequiredFields, got {spookey_error:?}");
        };
        assert_eq!(&*fields, &["cve-list-checkout-path"]);
    }

    #[test]
    fn mutually_exclusive_database_connection_sources_fail_config_parse() {
        let file = TempConfigFile::new(
            "server-address = 127.0.0.1:0\n\
            cve-list-checkout-path = /tmp/night-vision-test-cvelistV5\n\
            database-connection = sqlite::memory:\n\
            database-connection-file = /run/secrets/nv-server/database-url\n",
        );

        let error = Config::parse(file.path()).expect_err("exclusive database sources should fail");

        let ConfigLoadError::FailedToParseConfigFileFields(error_path, ConfigErrors(errors)) =
            error
        else {
            panic!("expected FailedToParseConfigFileFields, got {error:?}");
        };

        assert_eq!(error_path, file.path);
        assert_eq!(errors.len(), 1);
        let ConfigError::MutuallyExclusive { keys } = &errors[0] else {
            panic!(
                "expected MutuallyExclusive config error, got {:?}",
                errors[0]
            );
        };
        assert_eq!(keys[0], "database-connection");
        assert_eq!(keys[1], "database-connection-file");
    }

    #[cfg(unix)]
    #[test]
    fn config_parse_rejects_insecure_database_connection_file_permissions() {
        let secret_file = TempConfigFile::new("postgres://user:password@localhost:5432/nv\n");
        set_file_permissions(
            secret_file.path(),
            TestFilePermissions::UnixGroupOrWorldReadable,
        );
        let file = TempConfigFile::new(&format!(
            "server-address = 127.0.0.1:0\n\
            cve-list-checkout-path = /tmp/night-vision-test-cvelistV5\n\
            database-connection-file = {}\n",
            secret_file.path()
        ));

        let error = Config::parse(file.path()).expect_err("insecure secret file should fail");

        let ConfigLoadError::FailedToReadSecretFile(error_path, secret_error) = error else {
            panic!("expected FailedToReadSecretFile, got {error:?}");
        };
        assert_eq!(error_path, secret_file.path);
        assert!(matches!(
            secret_error,
            SecretFileError::InsecurePermissions(0o644)
        ));
    }

    #[cfg(windows)]
    #[test]
    fn config_parse_rejects_insecure_database_connection_file_permissions() {
        let secret_file = TempConfigFile::new("postgres://user:password@localhost:5432/nv\n");
        let _grant_read = GrantReadAclGuard::new(secret_file.path());
        let file = TempConfigFile::new(&format!(
            "server-address = 127.0.0.1:0\n\
            cve-list-checkout-path = /tmp/night-vision-test-cvelistV5\n\
            database-connection-file = {}\n",
            secret_file.path()
        ));

        let error =
            Config::parse(file.path()).expect_err("broadly-readable secret file should fail");

        let ConfigLoadError::FailedToReadSecretFile(error_path, secret_error) = error else {
            panic!("expected FailedToReadSecretFile, got {error:?}");
        };
        assert_eq!(error_path, secret_file.path);
        assert!(matches!(
            secret_error,
            SecretFileError::InsecurePermissions(0)
        ));
    }

    #[test]
    fn spookey_warnings_are_returned_as_config_field_errors() {
        let file = TempConfigFile::new(&format!(
            "{}\n{}",
            valid_required_config(),
            "unexpected-key = value",
        ));

        let error =
            Config::parse(file.path()).expect_err("unexpected key should fail config parse");

        let ConfigLoadError::FailedToParseConfigFileFields(error_path, ConfigErrors(errors)) =
            error
        else {
            panic!("expected FailedToParseConfigFileFields, got {error:?}");
        };

        assert_eq!(error_path, file.path);
        assert_eq!(errors.len(), 1);
        let ConfigError::Warning(warning) = &errors[0] else {
            panic!("expected Warning config error, got {:?}", errors[0]);
        };
        assert_eq!(warning.line_number, 4);
        let spookey::WarningKind::UnexpectedKey { key } = &warning.kind else {
            panic!("expected UnexpectedKey warning, got {warning:?}");
        };
        assert_eq!(&**key, "unexpected-key");
    }

    #[test]
    fn invalid_field_value_is_returned_as_str_parse_config_error() {
        let file = TempConfigFile::new(&format!(
            "{}\n{}",
            valid_required_config(),
            "database-max-connections = many",
        ));

        let error = Config::parse(file.path()).expect_err("invalid typed value should fail");

        let ConfigLoadError::FailedToParseConfigFileFields(error_path, ConfigErrors(errors)) =
            error
        else {
            panic!("expected FailedToParseConfigFileFields, got {error:?}");
        };

        assert_eq!(error_path, file.path);
        assert_eq!(errors.len(), 1);
        let ConfigError::StrParse(parse_error) = &errors[0] else {
            panic!("expected StrParse config error, got {:?}", errors[0]);
        };
        assert_eq!(&*parse_error.key, "database-max-connections");
        assert_eq!(&*parse_error.value, "many");
        assert!(parse_error.err.contains("invalid digit"));
    }

    #[test]
    fn dropshot_config_returns_field_error_for_invalid_server_address() {
        let file = TempConfigFile::new(&valid_required_config());
        let mut config = Config::parse(file.path()).expect("config should parse");
        config.server_address = "not an address".to_owned();

        let error = config
            .dropshot_config()
            .expect_err("invalid dropshot bind address should fail");

        let ConfigLoadError::FailedToParseConfigFileFields(error_path, ConfigErrors(errors)) =
            error
        else {
            panic!("expected FailedToParseConfigFileFields, got {error:?}");
        };

        assert_eq!(error_path, file.path);
        assert_eq!(errors.len(), 1);
        let ConfigError::StrParse(parse_error) = &errors[0] else {
            panic!("expected StrParse config error, got {:?}", errors[0]);
        };
        assert_eq!(&*parse_error.key, "server-address");
        assert_eq!(&*parse_error.value, "not an address");
    }

    #[test]
    fn redact_database_conn_redacts_password() {
        let conn = "postgres://nv-server:night-vision@postgres:5432/nv";

        assert_eq!(
            redact_database_conn_password(conn),
            "postgres://nv-server:<redacted>@postgres:5432/nv"
        );
    }

    #[test]
    fn redact_database_conn_preserves_connection_without_password() {
        let conn = "postgres://postgres:5432/nv";

        assert_eq!(redact_database_conn_password(conn), conn);
    }

    #[test]
    fn redact_database_conn_preserves_connection_without_userinfo() {
        let conn = "postgres://localhost/nv";

        assert_eq!(redact_database_conn_password(conn), conn);
    }
}
