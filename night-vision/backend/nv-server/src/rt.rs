//! Interface for configuring and working with the Tokio async runtime.

use crate::{config::Config, error::FatalError};
use std::time::Duration;

/// Our asynchronous runtime, which handled scheduling and execution of all async tasks.
///
/// This is a wrapper for `tokio::runtime::Runtime`, introduced for ergonomic reasons.
pub struct AsyncRuntime(tokio::runtime::Runtime);

impl AsyncRuntime {
    /// Construct a new async runtime using the given configuration.
    pub fn new(config: &Config) -> Result<Self, FatalError> {
        let mut builder = tokio::runtime::Builder::new_multi_thread();

        // Make sure to turn on IO and timers, otherwise it won't run at all.
        builder.enable_all();

        if let Some(worker_threads) = config.async_worker_threads {
            builder.worker_threads(worker_threads);
        }

        if let Some(thread_stack_size) = config.async_worker_thread_stack_size {
            builder.thread_stack_size(thread_stack_size);
        }

        if let Some(max_blocking_threads) = config.async_max_blocking_threads {
            builder.max_blocking_threads(max_blocking_threads);
        }

        if let Some(keep_alive) = config.async_blocking_thread_keep_alive {
            builder.thread_keep_alive(Duration::from_millis(keep_alive));
        }

        if let Some(global_queue_interval) = config.async_global_queue_interval {
            builder.global_queue_interval(global_queue_interval);
        }

        if let Some(event_interval) = config.async_event_interval {
            builder.event_interval(event_interval);
        }

        let runtime = builder
            .build()
            .map_err(FatalError::FailedToBuildTokioRuntime)?;

        Ok(AsyncRuntime(runtime))
    }

    /// Run a future to completion on the async runtime.
    pub fn block_on<F>(&self, fut: F) -> F::Output
    where
        F: Future,
    {
        self.0.block_on(fut)
    }
}
