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

pub struct Config {
    /// The address to bind the server to.
    ///
    /// The default is `127.0.0.1` at an arbitrary available port..
    pub server_addr: Option<String>,

    /// The maximum size in bytes for the request body.
    ///
    /// The default is 1024 bytes.
    pub default_request_body_max_bytes: Option<usize>,

    /// The default behavior for HTTP handler functions when clients disconnect early.
    pub default_early_disconnect_behavior: Option<EarlyDisconnectBehavior>,

    /// The number of async worker threads for Tokio to use.
    ///
    /// The default is equal to the number of CPU cores available on the system.
    pub async_worker_threads: Option<usize>,

    /// The size in bytes for stacks available to worker threads.
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
            "server-addr",
            "default-request-body-max-bytes",
            "default-early-disconnect-behavior",
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
                    .into_iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            );
            return Err(anyhow!(msg));
        }

        let mut value_errors = Vec::new();

        let server_addr = Self::parse_value(&kvs, "server-addr", &mut value_errors);

        let default_request_body_max_bytes =
            Self::parse_value(&kvs, "default-request-body-max-bytes", &mut value_errors);

        let default_early_disconnect_behavior =
            Self::parse_value(&kvs, "default-early-disconnect-behavior", &mut value_errors);

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
                "{} failed to parse values in config file '{}':\n{}",
                value_errors.len(),
                path,
                value_errors.join("\n")
            );
            return Err(anyhow!(msg));
        }

        let config = Config {
            server_addr,
            default_request_body_max_bytes,
            default_early_disconnect_behavior,
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
                        "line {}: failed to read line: {}",
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
                    "line {}: invalid line, expected 'key = value' format",
                    line_number + 1
                ));

                continue;
            };

            let key = key.trim().to_string();
            let value = value.trim().to_string();
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

        if let Some(addr) = &self.server_addr {
            config.bind_address = addr.parse()?;
        }

        if let Some(max_bytes) = self.default_request_body_max_bytes {
            config.default_request_body_max_bytes = max_bytes;
        }

        if let Some(behavior) = self.default_early_disconnect_behavior {
            config.default_handler_task_mode = behavior.into();
        }

        Ok(config)
    }
}

#[derive(Clone, Copy)]
pub enum EarlyDisconnectBehavior {
    CancelOnDisconnect,
    Detached,
}

impl FromStr for EarlyDisconnectBehavior {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.trim() {
            "cancel-on-disconnect" => Ok(Self::CancelOnDisconnect),
            "detached" => Ok(Self::Detached),
            _ => Err(anyhow::anyhow!(
                "invalid early disconnect behavior (must be 'cancel-on-disconnect' or 'detached'): {}",
                s
            )),
        }
    }
}

impl From<EarlyDisconnectBehavior> for HandlerTaskMode {
    fn from(value: EarlyDisconnectBehavior) -> Self {
        match value {
            EarlyDisconnectBehavior::CancelOnDisconnect => HandlerTaskMode::CancelOnDisconnect,
            EarlyDisconnectBehavior::Detached => HandlerTaskMode::Detached,
        }
    }
}
