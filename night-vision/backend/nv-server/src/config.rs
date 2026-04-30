//! Defines configuration for the Night Vision server.

use anyhow::{Context, Result, anyhow};
use camino::Utf8Path;
use dropshot::{ConfigDropshot, HandlerTaskMode};
use std::{
    collections::HashMap,
    fmt::Display,
    fs::File,
    io::{BufRead as _, BufReader},
    ops::Not,
    str::FromStr,
};

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
        let kvs = Self::parse_kvs(path)?;

        let known_keys = [
            "server-address",
            "http-request-body-max-bytes",
            "http-early-disconnect-behavior",
            "database-connection",
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
        ];

        let unexpected_keys = kvs
            .keys()
            .filter(|key| known_keys.contains(&key.as_str()).not())
            .collect::<Vec<_>>();

        if unexpected_keys.is_empty().not() {
            let msg = format!(
                "{} unknown keys in config file '{}':\n{}",
                unexpected_keys.len(),
                path,
                unexpected_keys
                    .iter()
                    .map(|s| format!("\t{}", s))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
            return Err(anyhow!(msg));
        }

        let mut value_errors = Vec::new();

        let server_addr = Self::parse_value(&kvs, "server-address", &mut value_errors);

        let http_request_body_max_bytes =
            Self::parse_value(&kvs, "http-request-body-max-bytes", &mut value_errors);

        let http_early_disconnect_behavior =
            Self::parse_value(&kvs, "http-early-disconnect-behavior", &mut value_errors);

        let database_conn = Self::parse_value(&kvs, "database-connection", &mut value_errors);

        let database_max_connections =
            Self::parse_value(&kvs, "database-max-connections", &mut value_errors);

        let database_min_connections =
            Self::parse_value(&kvs, "database-min-connections", &mut value_errors);

        let database_connect_timeout =
            Self::parse_value(&kvs, "database-connect-timeout", &mut value_errors);

        let database_idle_timeout =
            Self::parse_value(&kvs, "database-idle-timeout", &mut value_errors);

        let database_acquire_timeout =
            Self::parse_value(&kvs, "database-acquire-timeout", &mut value_errors);

        let database_max_lifetime =
            Self::parse_value(&kvs, "database-max-lifetime", &mut value_errors);

        let async_worker_threads =
            Self::parse_value(&kvs, "async-worker-threads", &mut value_errors);

        let async_worker_thread_stack_size =
            Self::parse_value(&kvs, "async-worker-thread-stack-size", &mut value_errors);

        let async_max_blocking_threads =
            Self::parse_value(&kvs, "async-max-blocking-threads", &mut value_errors);

        let async_blocking_thread_keep_alive =
            Self::parse_value(&kvs, "async-blocking-thread-keep-alive", &mut value_errors);

        let async_global_queue_interval =
            Self::parse_value(&kvs, "async-global-queue-interval", &mut value_errors);

        let async_event_interval =
            Self::parse_value(&kvs, "async-event-interval", &mut value_errors);

        if value_errors.is_empty().not() {
            let msg = format!(
                "{} values failed to parse in config file '{}':\n{}",
                value_errors.len(),
                path,
                value_errors
                    .iter()
                    .map(|s| format!("\t{}", s))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
            return Err(anyhow!(msg));
        }

        let mut missing_required = Vec::new();

        if server_addr.is_none() {
            missing_required.push("server-address");
        }

        if database_conn.is_none() {
            missing_required.push("database-connection");
        }

        if missing_required.is_empty().not() {
            let msg = format!(
                "{} missing required config value{}:\n{}",
                missing_required.len(),
                if missing_required.len() > 1 { "s" } else { "" },
                missing_required
                    .iter()
                    .map(|s| format!("\t{}", s))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            return Err(anyhow!(msg));
        }

        let config = Config {
            // PANIC SAFETY: We've already checked that it's Some above.
            server_address: server_addr.expect("server-address is required"),
            http_request_body_max_bytes,
            http_early_disconnect_behavior,
            // PANIC SAFETY: We've already checked that it's Some above.
            database_connection: database_conn.expect("database-connection is required"),
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
        };

        Ok(config)
    }

    /// Parse a key-value config file into a [`HashMap`] of string keys and string values.
    fn parse_kvs(path: &Utf8Path) -> Result<HashMap<String, String>> {
        let file = BufReader::new(
            File::open(path).context(format!("failed to open config file '{}'", path))?,
        );
        let mut kvs: HashMap<String, String> = HashMap::new();
        let mut parse_errors = Vec::new();

        for (line_number, line) in file.lines().enumerate() {
            let line = match line {
                Ok(line) => line,
                Err(err) => {
                    parse_errors.push(anyhow!(
                        "\tline {}: failed to read line: {}",
                        line_number + 1,
                        err
                    ));

                    continue;
                }
            };

            let line = line.trim();

            if line.is_empty() {
                continue;
            }

            if line.starts_with("#") {
                continue;
            }

            let Some((key, value)) = line.split_once('=') else {
                parse_errors.push(anyhow!(
                    "\tline {}: invalid line, expected 'key = value' format",
                    line_number + 1
                ));

                continue;
            };

            let key = key.trim().to_string();

            if key.is_empty() {
                parse_errors.push(anyhow!("\tline {}: key is empty", line_number + 1));

                continue;
            }

            let value = value.trim().to_string();

            // We permit empty values and treat them as the key/value pair not being present at all
            // (so the default value is used). This matches the behavior in Ghostty, and lets the
            // configuration file have empty values for all the keys so we can clearly document
            // what the keys are within the config file itself, without setting them or commenting
            // them out.
            if value.is_empty() {
                continue;
            }

            // If the value is quoted-wrapped, then remove the quotes before storing the value.
            if value.starts_with("\"") && value.ends_with("\"") {
                let value = value
                    .trim_start_matches('"')
                    .trim_end_matches('"')
                    .to_string();

                kvs.insert(key, value);
                continue;
            }

            kvs.insert(key, value);
        }

        if parse_errors.is_empty().not() {
            let msg = format!(
                "{} errors while parsing config file '{}':\n{}",
                parse_errors.len(),
                path,
                parse_errors
                    .iter()
                    .map(|e| e.to_string())
                    .collect::<Vec<_>>()
                    .join("\n")
            );

            return Err(anyhow!(msg));
        }

        Ok(kvs)
    }

    /// Parse a value from the config map, returning `None` if the key is not present,
    /// and if parsing fails adding it to the error vector for later reporting.
    fn parse_value<T: std::str::FromStr>(
        kvs: &HashMap<String, String>,
        key: &str,
        errors: &mut Vec<String>,
    ) -> Option<T>
    where
        <T as FromStr>::Err: Display,
    {
        match kvs.get(key) {
            Some(v) => match v.parse() {
                Ok(v) => Some(v),
                Err(e) => {
                    errors.push(format!("{}: {}", key, e));
                    None
                }
            },
            None => None,
        }
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
