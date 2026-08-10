use clap_verbosity_flag::VerbosityFilter;
use slog::{Drain, Level, Logger, OwnedKVList, Record};
use slog_async::Async;
use slog_term::{FullFormat, TermDecorator};
use std::sync::atomic::{AtomicBool, Ordering};

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
    logger_with_dynamic_debug(filter, None)
}

/// Build a new `slog::Logger` whose debug-level filtering can be controlled at
/// runtime.
///
/// When `debug_enabled` is `Some`, debug records are emitted whenever the
/// shared flag is set to `true`, even if the baseline `filter` would otherwise
/// suppress them. All other levels continue to follow the baseline filter.
///
/// If too many logs are sent at once, the asynchronous channel will overflow
/// and drop log entries.
///
/// PANIC: This calls `slog::Drain::fuse()`, which means that errors
/// produced by loggers will result in panics. In particular, it seems
/// possible for `slog_term` to produce `io::Error`s when writing
/// logs.
pub fn logger_with_dynamic_debug(
    filter: VerbosityFilter,
    debug_enabled: Option<&'static AtomicBool>,
) -> slog::Logger {
    let level = verbosity_filter_to_slog_level(filter);
    let decorator = TermDecorator::new().build();
    // PANIC: the calls to `.fuse()` here mean that errors produced by
    // loggers will result in panics. In particular, it seems possible
    // for `slog_term` to produce `io::Error`s when writing logs.
    let drain = FullFormat::new(decorator).build().fuse();
    let level_drain = DynamicDebugDrain::new(drain, level, debug_enabled).fuse();
    let async_drain = Async::new(level_drain).chan_size(1024).build().fuse();
    Logger::root(async_drain, slog::o!())
}

struct DynamicDebugDrain<D> {
    drain: D,
    baseline_level: Level,
    debug_enabled: Option<&'static AtomicBool>,
}

impl<D> DynamicDebugDrain<D> {
    fn new(drain: D, baseline_level: Level, debug_enabled: Option<&'static AtomicBool>) -> Self {
        Self {
            drain,
            baseline_level,
            debug_enabled,
        }
    }
}

impl<D> Drain for DynamicDebugDrain<D>
where
    D: Drain<Ok = ()>,
{
    type Ok = ();
    type Err = D::Err;

    fn log(&self, record: &Record<'_>, values: &OwnedKVList) -> Result<Self::Ok, Self::Err> {
        if should_log(record.level(), self.baseline_level, self.debug_enabled) {
            self.drain.log(record, values)?;
        }

        Ok(())
    }
}

fn should_log(
    record_level: Level,
    baseline_level: Level,
    debug_enabled: Option<&AtomicBool>,
) -> bool {
    if record_level == Level::Debug {
        return debug_enabled.is_some_and(|flag| flag.load(Ordering::Relaxed))
            || baseline_allows_level(record_level, baseline_level);
    }

    baseline_allows_level(record_level, baseline_level)
}

fn baseline_allows_level(record_level: Level, baseline_level: Level) -> bool {
    level_rank(record_level) <= level_rank(baseline_level)
}

fn level_rank(level: Level) -> u8 {
    match level {
        Level::Critical => 0,
        Level::Error => 1,
        Level::Warning => 2,
        Level::Info => 3,
        Level::Debug => 4,
        Level::Trace => 5,
    }
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

#[cfg(test)]
mod tests {
    use super::{baseline_allows_level, should_log};
    use slog::Level;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn should_log_suppresses_debug_until_enabled() {
        let debug_enabled = AtomicBool::new(false);

        assert!(!should_log(Level::Debug, Level::Info, Some(&debug_enabled)));
        assert!(should_log(Level::Info, Level::Info, Some(&debug_enabled)));

        debug_enabled.store(true, std::sync::atomic::Ordering::Relaxed);

        assert!(should_log(Level::Debug, Level::Info, Some(&debug_enabled)));
        assert!(should_log(Level::Error, Level::Info, Some(&debug_enabled)));
    }

    #[test]
    fn should_log_keeps_non_debug_levels_unchanged() {
        let debug_enabled = AtomicBool::new(false);

        assert!(!should_log(Level::Info, Level::Error, Some(&debug_enabled)));
        assert!(should_log(Level::Error, Level::Error, Some(&debug_enabled)));
        assert!(!should_log(
            Level::Warning,
            Level::Error,
            Some(&debug_enabled)
        ));
    }

    #[test]
    fn should_log_without_dynamic_debug_matches_baseline_behavior() {
        assert!(should_log(Level::Info, Level::Info, None));
        assert!(should_log(Level::Debug, Level::Debug, None));
        assert!(!should_log(Level::Debug, Level::Info, None));
        assert!(!should_log(Level::Trace, Level::Info, None));
    }

    #[test]
    fn baseline_allows_level_matches_expected_slog_ordering() {
        assert!(baseline_allows_level(Level::Critical, Level::Info));
        assert!(baseline_allows_level(Level::Error, Level::Info));
        assert!(baseline_allows_level(Level::Info, Level::Info));
        assert!(!baseline_allows_level(Level::Debug, Level::Info));
        assert!(!baseline_allows_level(Level::Trace, Level::Info));
    }
}
