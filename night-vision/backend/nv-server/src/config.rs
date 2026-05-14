//! Defines configuration for the Night Vision server.

use crate::error::{ErrorSourceIterator, FatalError};
use camino::{Utf8Path, Utf8PathBuf};
use dropshot::{ConfigDropshot, HandlerTaskMode};
use itertools::Itertools;
use std::{
    fmt::{Debug, Display},
    fs::File,
    io::BufReader,
    net::AddrParseError,
    str::FromStr,
};

/// The default configuration file, relative to the root of the workspace. This is used by the CLI
/// if `-c`/`--config` is not set.
pub const DEFAULT_CONFIG_FILE: &str = "nv-server.spookey";

/// Configuration for the Night Vision server.
#[derive(Debug)]
pub struct Config {
    /// The path to the configuration file.
    config_file_path: Utf8PathBuf,

    /// The address to bind the server to.
    ///
    /// The default is `127.0.0.1` at an arbitrary available port..
    pub server_address: String,

    /// The maximum size in bytes for the request body.
    ///
    /// The default is 1024 bytes.
    pub http_request_body_max_bytes: Option<usize>,

    /// The default behavior for HTTP handler functions when clients disconnect early.
    pub http_early_disconnect_behavior: Option<EarlyDisconnectBehavior>,

    /// String used to connect to the database.
    pub database_connection: String,

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
    pub fn parse(path: &Utf8Path) -> Result<Config, FatalError> {
        let parsed = spookey::parse(
            spookey::ParseConfig {
                required_keys: vec!["server-address", "database-connection"],
                optional_keys: vec![
                    "http-request-body-max-bytes",
                    "http-early-disconnect-behavior",
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
                ],
            },
            BufReader::new(
                File::open(path).map_err(|e| FatalError::FailedToOpenConfigFile(path.into(), e))?,
            ),
        )
        .map_err(|e| FatalError::FailedToParseConfigFile(path.into(), e))?;

        // Construct the config object, parsing fields into the proper types and collecting errors.
        let mut errors = Vec::new();
        let config = Config {
            config_file_path: path.into(),
            // PANIC SAFETY: We've already checked that it's Some above.
            server_address: parse_value(&parsed, "server-address", &mut errors)
                .expect("server-address is required"),
            http_request_body_max_bytes: parse_value(
                &parsed,
                "http-request-body-max-bytes",
                &mut errors,
            ),
            http_early_disconnect_behavior: parse_value(
                &parsed,
                "http-early-disconnect-behavior",
                &mut errors,
            ),
            // PANIC SAFETY: We've already checked that it's Some above.
            database_connection: parse_value(&parsed, "database-connection", &mut errors)
                .expect("database-connection is required"),
            database_max_connections: parse_value(&parsed, "database-max-connections", &mut errors),
            database_min_connections: parse_value(&parsed, "database-min-connections", &mut errors),
            database_connect_timeout: parse_value(&parsed, "database-connect-timeout", &mut errors),
            database_idle_timeout: parse_value(&parsed, "database-idle-timeout", &mut errors),
            database_acquire_timeout: parse_value(&parsed, "database-acquire-timeout", &mut errors),
            database_max_lifetime: parse_value(&parsed, "database-max-lifetime", &mut errors),
            async_worker_threads: parse_value(&parsed, "async-worker-threads", &mut errors),
            async_worker_thread_stack_size: parse_value(
                &parsed,
                "async-worker-thread-stack-size",
                &mut errors,
            ),
            async_max_blocking_threads: parse_value(
                &parsed,
                "async-max-blocking-threads",
                &mut errors,
            ),
            async_blocking_thread_keep_alive: parse_value(
                &parsed,
                "async-blocking-thread-keep-alive",
                &mut errors,
            ),
            async_global_queue_interval: parse_value(
                &parsed,
                "async-global-queue-interval",
                &mut errors,
            ),
            async_event_interval: parse_value(&parsed, "async-event-interval", &mut errors),
        };

        check_errors(path, parsed.warnings, errors)?;

        Ok(config)
    }

    /// Get the Dropshot server configuration items out of the overall config.
    ///
    /// The `ConfigDropshot` type is the type that Dropshot expects for its own configuration,
    /// so we take the fields that apply to Dropshot and pull them out here.
    pub fn dropshot_config(&self) -> Result<ConfigDropshot, FatalError> {
        let mut config = ConfigDropshot {
            bind_address: self.server_address.parse().map_err(|e: AddrParseError| {
                FatalError::FailedToParseConfigFileFields(
                    self.config_file_path.clone(),
                    ConfigErrors(vec![ConfigError::StrParse(StrParseError {
                        key: "server-address".to_string().into_boxed_str(),
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
}

/// Write a "separator" line of 80 dashes, used at the start and end of the config report.
macro_rules! write_report_separator {
    ($f:ident) => {
        write!($f, "{:-^80}\n", "")
    };
}

/// Write a single line of the config report.
macro_rules! write_report_line {
    // Note that the "32" value is set by-hand based on the length in characters of the longest
    // configuration key we accept. If we introduce longer configuration keys in the future, this will
    // likely need to be bumped.
    ($f:ident, $key:literal, $value:expr) => {
        write!($f, "{:>32}: {}\n", $key, $value)
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
        write_report_line!(f, "database-connection", &self.database_connection)?;

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

        write_report_separator!(f)?;

        Ok(())
    }
}

/// Parse a value from the config map, returning `None` if the value is unset.
///
/// This also records any parsing failures as warnings.
fn parse_value<T: std::str::FromStr>(
    results: &spookey::ParseResult,
    key: &str,
    errors: &mut Vec<StrParseError>,
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
                errors.push(StrParseError {
                    key: key.to_string().into_boxed_str(),
                    value: value.clone().into_boxed_str(),
                    // Make sure we package up the full error chain, not just the top-level error.
                    err: err
                        .sources_iter()
                        .map(ToString::to_string)
                        .join(": ")
                        .into_boxed_str(),
                });
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

/// Check collected warnings and field parsing errors, bundling them in a `FatalError` to report.
fn check_errors(
    path: &Utf8Path,
    parsed_warnings: Vec<spookey::Warning>,
    value_parsing_errors: Vec<StrParseError>,
) -> Result<(), FatalError> {
    let num_errors = parsed_warnings.len() + value_parsing_errors.len();

    if num_errors == 0 {
        return Ok(());
    }

    let mut errors = vec![];

    for warning in parsed_warnings {
        errors.push(ConfigError::Warning(warning));
    }

    for error in value_parsing_errors {
        errors.push(ConfigError::StrParse(error));
    }

    Err(FatalError::FailedToParseConfigFileFields(
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
            EarlyDisconnectBehavior::Cancel => write!(f, "cancel"),
            EarlyDisconnectBehavior::Continue => write!(f, "continue"),
        }
    }
}

impl From<EarlyDisconnectBehavior> for HandlerTaskMode {
    fn from(value: EarlyDisconnectBehavior) -> Self {
        match value {
            EarlyDisconnectBehavior::Cancel => HandlerTaskMode::CancelOnDisconnect,
            EarlyDisconnectBehavior::Continue => HandlerTaskMode::Detached,
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
}

impl Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Warning(w) => write!(f, "{}", w),
            ConfigError::StrParse(e) => write!(f, "{}", e),
        }
    }
}

/// Bundles up a collection of [`ConfigError`]s.
#[derive(Debug)]
pub struct ConfigErrors(Vec<ConfigError>);

impl Display for ConfigErrors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = self.0.iter().map(ToString::to_string).join(", ");
        write!(f, "config errors: {}", msg)
    }
}

impl std::error::Error for ConfigErrors {}
