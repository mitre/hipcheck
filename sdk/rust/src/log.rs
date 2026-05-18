// SPDX-License-Identifier: Apache-2.0

use hipcheck_common::types::LogLevel;

/// Initializes a `tracing-subscriber` for plugin logging and forwards logs to Hipcheck core.
///
/// Initializes a `tracing-subscriber` which writes log messages produced via the `tracing` crate macros to stdout/stderr.
/// These plugin logs are then piped to Hipcheck core's stdout/stderr log output during execution.
///
/// `tracing-subscriber` uses an `EnvFilter` to filter logs to up to the log-level passed as argument `log-level`
/// to the plugin by `HC Core`.
///
/// log_forwarding() is enabled as a default feature, but could be disabled and replaced by setting `default-features = false`
/// in the `dependencies` section of a plugin's `Cargo.toml` prior to compilation.
#[cfg(feature = "log_forwarding")]
pub fn init_tracing_logger(log_level: LogLevel) {
	tracing_subscriber::fmt()
		.json()
		.with_writer(std::io::stderr)
		.with_env_filter(log_level.to_string())
		.init();
}
