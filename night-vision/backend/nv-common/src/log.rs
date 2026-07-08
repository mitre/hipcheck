use clap_verbosity_flag::VerbosityFilter;
use slog::{Drain as _, Level, LevelFilter, Logger};
use slog_async::Async;
use slog_term::{FullFormat, TermDecorator};

/// Build a new `slog::Logger`.
///
/// Logs are written asynchronously by a separate thread using `slog-async`,
/// to be written to a terminal using `slog-term`.
///
/// If too many logs are sent at once, the asynchronous channel will overflow
/// and drop log entries.
///
/// PANIC: This calls `slog::Drain::fuse()`, which means that errors
/// produced by loggers will result in panics. In particular, it seems
/// possible for `slog_term` to produce `io::Error`s when writing
/// logs.
pub fn logger(filter: VerbosityFilter) -> slog::Logger {
    let level = verbosity_filter_to_slog_level(filter);
    let decorator = TermDecorator::new().build();
    // PANIC: the calls to `.fuse()` here mean that errors produced by
    // loggers will result in panics. In particular, it seems possible
    // for `slog_term` to produce `io::Error`s when writing logs.
    let drain = FullFormat::new(decorator).build().fuse();
    let level_drain = LevelFilter(drain, level).fuse();
    let async_drain = Async::new(level_drain).chan_size(1024).build().fuse();
    Logger::root(async_drain, slog::o!())
}

/// Convert a Verbosity Filter from `clap-verbosity-flag` to a
/// `slog::Level`. This is necessary since `clap-verbosity-flag` as of
/// 3.0.4 does not support `slog`.
fn verbosity_filter_to_slog_level(filter: VerbosityFilter) -> slog::Level {
    use VerbosityFilter::{Debug, Error, Info, Off, Trace, Warn};
    match filter {
        Off => Level::Critical,
        Error => Level::Error,
        Warn => Level::Warning,
        Info => Level::Info,
        Debug => Level::Debug,
        Trace => Level::Trace,
    }
}
