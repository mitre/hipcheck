//! Defines configuration for the Night Vision server.

use anyhow::{Context as _, Result};
use camino::Utf8Path;
use dropshot::{ConfigDropshot, HandlerTaskMode};
use std::{fmt::Display, fs::File, io::BufReader, ops::Not, str::FromStr};

/// Configuration for the Night Vision server.
#[derive(Debug)]
pub struct Config {
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
    /// If not provided, then `nv-server` will look for a file called `config.nv` in the current
    /// directory.
    ///
    /// Parsing will fail on unknown keys, or on keys that fail to parse. This is purposefully
    /// pretty strict; these are server configuration items, and a malformed key or value should
    /// be considered a configuration failure.
    pub fn parse(path: &Utf8Path) -> Result<Config> {
        let config = spookey::ParseConfig {
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
        };

        let reader = BufReader::new(
            File::open(path).context(format!("failed to open config file '{}'", path))?,
        );

        let parsed = spookey::parse(config, reader)?;

        let mut warnings = Vec::new();

        let config = Config {
            // PANIC SAFETY: We've already checked that it's Some above.
            server_address: parse_value(&parsed, "server-address", &mut warnings)
                .expect("server-address is required"),
            http_request_body_max_bytes: parse_value(
                &parsed,
                "http-request-body-max-bytes",
                &mut warnings,
            ),
            http_early_disconnect_behavior: parse_value(
                &parsed,
                "http-early-disconnect-behavior",
                &mut warnings,
            ),
            // PANIC SAFETY: We've already checked that it's Some above.
            database_connection: parse_value(&parsed, "database-connection", &mut warnings)
                .expect("database-connection is required"),
            database_max_connections: parse_value(
                &parsed,
                "database-max-connections",
                &mut warnings,
            ),
            database_min_connections: parse_value(
                &parsed,
                "database-min-connections",
                &mut warnings,
            ),
            database_connect_timeout: parse_value(
                &parsed,
                "database-connect-timeout",
                &mut warnings,
            ),
            database_idle_timeout: parse_value(&parsed, "database-idle-timeout", &mut warnings),
            database_acquire_timeout: parse_value(
                &parsed,
                "database-acquire-timeout",
                &mut warnings,
            ),
            database_max_lifetime: parse_value(&parsed, "database-max-lifetime", &mut warnings),
            async_worker_threads: parse_value(&parsed, "async-worker-threads", &mut warnings),
            async_worker_thread_stack_size: parse_value(
                &parsed,
                "async-worker-thread-stack-size",
                &mut warnings,
            ),
            async_max_blocking_threads: parse_value(
                &parsed,
                "async-max-blocking-threads",
                &mut warnings,
            ),
            async_blocking_thread_keep_alive: parse_value(
                &parsed,
                "async-blocking-thread-keep-alive",
                &mut warnings,
            ),
            async_global_queue_interval: parse_value(
                &parsed,
                "async-global-queue-interval",
                &mut warnings,
            ),
            async_event_interval: parse_value(&parsed, "async-event-interval", &mut warnings),
        };

        report_warnings(path, &parsed.warnings, &warnings);

        Ok(config)
    }

    /// Get the Dropshot server configuration items out of the overall config.
    pub fn dropshot_config(&self) -> Result<ConfigDropshot> {
        let mut config = ConfigDropshot::default();

        config.bind_address = self.server_address.parse()?;

        if let Some(max_bytes) = self.http_request_body_max_bytes {
            config.default_request_body_max_bytes = max_bytes;
        }

        if let Some(behavior) = self.http_early_disconnect_behavior {
            config.default_handler_task_mode = behavior.into();
        }

        Ok(config)
    }

    /// Get a friendly report on the configuration being used.
    pub fn report(&self) -> String {
        let mut report = format!("{:-^80}\n", "");

        report.push_str(&self.report_line("server-address", &self.server_address));
        report.push_str(&self.report_line("database-connection", &self.database_connection));

        if let Some(max_bytes) = self.http_request_body_max_bytes {
            report
                .push_str(&self.report_line("http-request-body-max-bytes", &max_bytes.to_string()));
        }

        if let Some(behavior) = self.http_early_disconnect_behavior {
            report.push_str(
                &self.report_line("http-early-disconnect-behavior", &behavior.to_string()),
            );
        }

        if let Some(max_connections) = self.database_max_connections {
            report.push_str(
                &self.report_line("database-max-connections", &max_connections.to_string()),
            );
        }

        if let Some(min_connections) = self.database_min_connections {
            report.push_str(
                &self.report_line("database-min-connections", &min_connections.to_string()),
            );
        }

        if let Some(connect_timeout) = self.database_connect_timeout {
            report.push_str(
                &self.report_line("database-connect-timeout", &connect_timeout.to_string()),
            );
        }

        if let Some(idle_timeout) = self.database_idle_timeout {
            report.push_str(&self.report_line("database-idle-timeout", &idle_timeout.to_string()));
        }

        if let Some(acquire_timeout) = self.database_acquire_timeout {
            report.push_str(
                &self.report_line("database-acquire-timeout", &acquire_timeout.to_string()),
            );
        }

        if let Some(max_lifetime) = self.database_max_lifetime {
            report.push_str(&self.report_line("database-max-lifetime", &max_lifetime.to_string()));
        }

        if let Some(async_worker_threads) = self.async_worker_threads {
            report.push_str(
                &self.report_line("async-worker-threads", &async_worker_threads.to_string()),
            );
        }

        if let Some(async_worker_thread_stack_size) = self.async_worker_thread_stack_size {
            report.push_str(&self.report_line(
                "async-worker-thread-stack-size",
                &async_worker_thread_stack_size.to_string(),
            ));
        }

        if let Some(async_max_blocking_threads) = self.async_max_blocking_threads {
            report.push_str(&self.report_line(
                "async-max-blocking-threads",
                &async_max_blocking_threads.to_string(),
            ));
        }

        if let Some(async_blocking_thread_keep_alive) = self.async_blocking_thread_keep_alive {
            report.push_str(&self.report_line(
                "async-blocking-thread-keep-alive",
                &async_blocking_thread_keep_alive.to_string(),
            ));
        }

        if let Some(async_global_queue_interval) = self.async_global_queue_interval {
            report.push_str(&self.report_line(
                "async-global-queue-interval",
                &async_global_queue_interval.to_string(),
            ));
        }

        if let Some(async_event_interval) = self.async_event_interval {
            report.push_str(
                &self.report_line("async-event-interval", &async_event_interval.to_string()),
            );
        }

        report.push_str(&format!("{:-^80}\n", ""));

        report
    }

    fn report_line(&self, key: &str, value: &str) -> String {
        format!("{:>32}: {}\n", key, value)
    }
}

/// Parse a value from the config map, returning `None` if the value is unset.
///
/// This also records any parsing failures as warnings.
fn parse_value<T: std::str::FromStr>(
    results: &spookey::ParseResult,
    key: &str,
    warnings: &mut Vec<String>,
) -> Option<T>
where
    <T as FromStr>::Err: Display,
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
        (None, Some(Some(value))) | (Some(value), None) => value
            .parse()
            .map_err(|err| {
                warnings.push(format!("{}: {}", key, err));
            })
            .ok(),
        // Known key, somehow found in both "required" and "optional" maps.
        (Some(_), Some(_)) => unreachable!(
            "spookey doesn't permit a single key to be in `required_keys` and `optional_keys`"
        ),
    }
}

fn report_warnings(
    path: &Utf8Path,
    parsed_warnings: &[spookey::Warning],
    value_parsing_warnings: &[String],
) {
    let mut msg = String::new();

    if parsed_warnings.is_empty().not() {
        msg.push_str(&format!(
            "{} warnings found in config file '{}':\n{}",
            parsed_warnings.len(),
            path,
            parsed_warnings
                .iter()
                .map(|s| format!("\t{}", s))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }

    if value_parsing_warnings.is_empty().not() {
        msg.push_str(&format!(
            "{} values failed to parse in config file '{}':\n{}",
            value_parsing_warnings.len(),
            path,
            value_parsing_warnings
                .iter()
                .map(|s| format!("\t{}", s))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }

    eprintln!("{}", msg);
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
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "cancel" => Ok(Self::Cancel),
            "continue" => Ok(Self::Continue),
            _ => Err(anyhow::anyhow!(
                "invalid early disconnect behavior (must be 'cancel' or 'continue'): {}",
                s
            )),
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
