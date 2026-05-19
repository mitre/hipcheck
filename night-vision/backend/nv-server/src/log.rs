use crate::{env::Env, error::FatalError};
use dropshot::{ConfigLogging, ConfigLoggingLevel};

/// Try to build a new `slog::Logger`.
pub fn logger(env: &Env) -> Result<slog::Logger, FatalError> {
    // TODO: Consider replacing this with our own logger setup. For now this uses Dropshot's.
    ConfigLogging::StderrTerminal {
        level: ConfigLoggingLevel::Info,
    }
    .to_logger(env.bin_name())
    .map_err(FatalError::FailedToInitializeLogger)
}
