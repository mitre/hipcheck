use slog::Logger;
#[cfg(unix)]
use slog::info;
#[cfg(unix)]
use std::sync::atomic::Ordering;
use std::sync::{Arc, OnceLock, atomic::AtomicBool};
use tokio::task::JoinHandle;

static DEBUG_MODE: OnceLock<Arc<AtomicBool>> = OnceLock::new();

/// Return the process-wide debug-mode flag.
pub fn debug_mode_flag() -> &'static AtomicBool {
    DEBUG_MODE
        .get_or_init(|| Arc::new(AtomicBool::new(false)))
        .as_ref()
}

/// Return whether process-wide debug mode is currently enabled.
#[cfg_attr(not(test), expect(dead_code, reason = "Only used in tests, for now"))]
#[cfg(unix)]
pub fn debug_mode_enabled() -> bool {
    debug_mode_flag().load(Ordering::Relaxed)
}

#[cfg(unix)]
fn toggle_debug_mode() -> bool {
    let was_enabled = debug_mode_flag().fetch_xor(true, Ordering::Relaxed);
    !was_enabled
}

/// Toggle debug logging in response to a received `SIGUSR1`.
#[cfg(unix)]
pub fn handle_sigusr1(log: &Logger) -> bool {
    let enabled = toggle_debug_mode();
    info!(log, "toggled debug logging via SIGUSR1"; "debug_enabled" => enabled);
    enabled
}

/// Spawn a background task that toggles debug logging whenever the process
/// receives `SIGUSR1`.
#[cfg(unix)]
pub fn spawn_sigusr1_listener(log: Logger) -> JoinHandle<()> {
    tokio::spawn(async move {
        use tokio::signal::unix::{SignalKind, signal};

        match signal(SignalKind::user_defined1()) {
            Ok(mut signals) => {
                while signals.recv().await.is_some() {
                    handle_sigusr1(&log);
                }
            }
            Err(error) => {
                slog::warn!(
                    log,
                    "failed to install SIGUSR1 listener; debug-log toggling unavailable";
                    "error" => error.to_string()
                );
            }
        }
    })
}

/// Spawn a no-op background task on non-Unix platforms, where `SIGUSR1`
/// signal handling is unavailable.
#[cfg(not(unix))]
pub fn spawn_sigusr1_listener(_log: Logger) -> JoinHandle<()> {
    tokio::spawn(async {})
}

#[cfg(test)]
mod tests {
    use super::debug_mode_flag;
    #[cfg(unix)]
    use super::{debug_mode_enabled, handle_sigusr1};
    #[cfg(unix)]
    use slog::Logger;
    use std::sync::atomic::Ordering;
    use std::sync::{Mutex, MutexGuard};

    static DEBUG_MODE_TEST_MUTEX: Mutex<()> = Mutex::new(());

    #[cfg(unix)]
    fn discard_logger() -> Logger {
        Logger::root(slog::Discard, slog::o!())
    }

    fn debug_mode_test_lock() -> MutexGuard<'static, ()> {
        DEBUG_MODE_TEST_MUTEX
            .lock()
            .expect("debug mode test mutex should not be poisoned")
    }

    fn reset_debug_mode() {
        debug_mode_flag().store(false, Ordering::Relaxed);
    }

    #[test]
    fn debug_mode_flag_is_process_wide() {
        let _guard = debug_mode_test_lock();
        reset_debug_mode();

        let first = debug_mode_flag();
        let second = debug_mode_flag();

        assert!(std::ptr::eq(first, second));
    }

    #[test]
    #[cfg(unix)]
    fn handle_sigusr1_toggles_debug_mode_on_and_off() {
        let _guard = debug_mode_test_lock();
        let log = discard_logger();

        reset_debug_mode();
        assert!(!debug_mode_enabled());

        assert!(handle_sigusr1(&log));
        assert!(debug_mode_enabled());

        assert!(!handle_sigusr1(&log));
        assert!(!debug_mode_enabled());
    }
}
