//! Defines configuration for the Night Vision server.

use crate::{
    error::{ErrorSourceIterator, FatalError},
    secret::{SecretSource, SecretSourceKind},
};
use camino::{Utf8Path, Utf8PathBuf};
use dropshot::{ConfigDropshot, HandlerTaskMode};
use itertools::Itertools;
use secrecy::SecretString;
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

    /// Source for the string used to connect to the database.
    database_connection_source: SecretSourceKind,

    /// String used to connect to the database.
    database_connection: SecretString,

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
                // While we *do* require some form of database connection information, either via
                // `database-connection` or `database-connection-file`, they're both listed as
                // optional here because we validate that only one of them is present after
                // parsing with `spookey`.
                required_keys: vec!["server-address"],
                optional_keys: vec![
                    "http-request-body-max-bytes",
                    "http-early-disconnect-behavior",
                    "database-connection",
                    "database-connection-file",
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

        // PANIC SAFETY: We've already checked that it's Some above.
        let server_address: String = parse_value(&parsed, "server-address", &mut errors)
            .expect("server-address is required");
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

        check_errors(path, parsed.warnings, errors)?;

        let (database_connection_source, database_connection) =
            resolve_database_connection_source(database_connection_source)?;

        let config = Config {
            config_file_path: path.into(),
            server_address,
            http_request_body_max_bytes,
            http_early_disconnect_behavior,
            database_connection_source,
            database_connection,
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

    /// Get the configured database connection string.
    pub fn database_connection(&self) -> &SecretString {
        &self.database_connection
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
        write_report_line!(f, "database-connection", &self.database_connection_source)?;

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
                    key: key.to_string().into_boxed_str(),
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
) -> Result<(SecretSourceKind, SecretString), FatalError> {
    source
        .resolve()
        .map(|secret| secret.into_parts())
        .map_err(|err| FatalError::FailedToReadSecretFile(err.path, err.error))
}

/// Check collected warnings and field parsing errors, bundling them in a `FatalError` to report.
fn check_errors(
    path: &Utf8Path,
    parsed_warnings: Vec<spookey::Warning>,
    value_parsing_errors: Vec<ConfigError>,
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
        errors.push(error);
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
    MissingOneOf { keys: Box<[&'static str; 2]> },
    MutuallyExclusive { keys: Box<[&'static str; 2]> },
}

impl Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Warning(w) => write!(f, "{}", w),
            ConfigError::StrParse(e) => write!(f, "{}", e),
            ConfigError::MissingOneOf { keys } => write!(
                f,
                "exactly one of '{}' or '{}' must be configured",
                keys[0], keys[1]
            ),
            ConfigError::MutuallyExclusive { keys } => {
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
        write!(f, "config errors: {}", msg)
    }
}

impl std::error::Error for ConfigErrors {}

#[cfg(test)]
mod tests {
    use crate::secret::SecretFileError;
    use secrecy::ExposeSecret;
    use std::{
        fs,
        sync::atomic::{AtomicUsize, Ordering},
    };

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

    #[cfg(unix)]
    fn set_file_mode(path: &Utf8Path, mode: u32) {
        let permissions = std::os::unix::fs::PermissionsExt::from_mode(mode);
        fs::set_permissions(path, permissions).expect("failed to set test file permissions");
    }

    #[cfg(not(unix))]
    fn set_file_mode(_path: &Utf8Path, _mode: u32) {}

    fn restrict_secret_file_permissions(path: &Utf8Path) {
        set_file_mode(path, 0o600);
    }

    fn redact_database_conn_password(conn: &str) -> String {
        let Some((scheme, rest)) = conn.split_once("://") else {
            return conn.to_string();
        };

        let Some((userinfo, host_and_path)) = rest.split_once('@') else {
            return conn.to_string();
        };

        let Some((user, _password)) = userinfo.rsplit_once(':') else {
            return conn.to_string();
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
            [
                format!("server-address = {}", self.server_address),
                format!("database-connection = {}", self.database_connection),
                self.extra_config.to_string(),
            ]
            .join("\n")
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
    }

    #[test]
    fn parses_database_connection_file_from_spookey_config() {
        let secret_file = TempConfigFile::new("postgres://user:password@localhost:5432/nv\n");
        restrict_secret_file_permissions(secret_file.path());
        let file = TempConfigFile::new(&format!(
            "server-address = 127.0.0.1:0\n\
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
    fn config_display_redacts_inline_database_connection() {
        let file = TempConfigFile::new(
            "server-address = 127.0.0.1:0\n\
            database-connection = postgres://user:password@localhost:5432/nv\n",
        );

        let config = Config::parse(file.path()).expect("config should parse");
        let report = format!("{}", config);

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
            database-connection-file = {}\n",
            secret_file.path()
        ));

        let config = Config::parse(file.path()).expect("config should parse");
        let report = format!("{}", config);

        assert!(report.contains("<redacted file-backed secret>"));
        assert!(!report.contains(secret_file.path().as_str()));
        assert!(!report.contains("password"));
        assert!(!report.contains("postgres://user:password@localhost:5432/nv"));
    }

    #[test]
    fn invalid_typed_field_values_are_returned_as_str_parse_config_errors() {
        let cases = [
            ("http-request-body-max-bytes", "many", "invalid digit"),
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
        ];

        for (key, value, expected_error) in cases {
            let file =
                TempConfigFile::new(&format!("{}\n{} = {}", valid_required_config(), key, value,));

            let error =
                Config::parse(file.path()).expect_err(&format!("{key} = {value} should fail"));

            let FatalError::FailedToParseConfigFileFields(error_path, ConfigErrors(errors)) = error
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

        let FatalError::FailedToOpenConfigFile(error_path, io_error) = error else {
            panic!("expected FailedToOpenConfigFile, got {error:?}");
        };

        assert_eq!(error_path, path);
        assert_eq!(io_error.kind(), std::io::ErrorKind::NotFound);
    }

    #[test]
    fn missing_database_connection_source_returns_config_field_error() {
        let file = TempConfigFile::new("server-address = 127.0.0.1:0\n");

        let error =
            Config::parse(file.path()).expect_err("missing database connection should fail");

        let FatalError::FailedToParseConfigFileFields(error_path, ConfigErrors(errors)) = error
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
        let file = TempConfigFile::new("database-connection = sqlite::memory:\n");

        let error = Config::parse(file.path()).expect_err("missing required key should fail");

        let FatalError::FailedToParseConfigFile(error_path, spookey_error) = error else {
            panic!("expected FailedToParseConfigFile, got {error:?}");
        };

        assert_eq!(error_path, file.path);
        let spookey::Error::MissingRequiredFields(fields) = spookey_error else {
            panic!("expected MissingRequiredFields, got {spookey_error:?}");
        };
        assert_eq!(&*fields, &["server-address"]);
    }

    #[test]
    fn mutually_exclusive_database_connection_sources_fail_config_parse() {
        let file = TempConfigFile::new(
            "server-address = 127.0.0.1:0\n\
            database-connection = sqlite::memory:\n\
            database-connection-file = /run/secrets/nv-server/database-url\n",
        );

        let error = Config::parse(file.path()).expect_err("exclusive database sources should fail");

        let FatalError::FailedToParseConfigFileFields(error_path, ConfigErrors(errors)) = error
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
        set_file_mode(secret_file.path(), 0o644);
        let file = TempConfigFile::new(&format!(
            "server-address = 127.0.0.1:0\n\
            database-connection-file = {}\n",
            secret_file.path()
        ));

        let error = Config::parse(file.path()).expect_err("insecure secret file should fail");

        let FatalError::FailedToReadSecretFile(error_path, secret_error) = error else {
            panic!("expected FailedToReadSecretFile, got {error:?}");
        };
        assert_eq!(error_path, secret_file.path);
        assert!(matches!(
            secret_error,
            SecretFileError::InsecurePermissions(0o644)
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

        let FatalError::FailedToParseConfigFileFields(error_path, ConfigErrors(errors)) = error
        else {
            panic!("expected FailedToParseConfigFileFields, got {error:?}");
        };

        assert_eq!(error_path, file.path);
        assert_eq!(errors.len(), 1);
        let ConfigError::Warning(warning) = &errors[0] else {
            panic!("expected Warning config error, got {:?}", errors[0]);
        };
        assert_eq!(warning.line_number, 3);
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

        let FatalError::FailedToParseConfigFileFields(error_path, ConfigErrors(errors)) = error
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
        config.server_address = "not an address".to_string();

        let error = config
            .dropshot_config()
            .expect_err("invalid dropshot bind address should fail");

        let FatalError::FailedToParseConfigFileFields(error_path, ConfigErrors(errors)) = error
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
