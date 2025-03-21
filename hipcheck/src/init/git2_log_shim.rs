// SPDX-License-Identifier: Apache-2.0

//! Log shim function to redirect [git2] trace messages to [log].

use crate::hc_error;

/// Shim the [git2] crate's tracing infrastructure with calls to the [log] crate which we use.
pub fn git2_set_trace_log_shim() {
	git2::trace_set(git2::TraceLevel::Trace, |level, msg| {
		use git2::TraceLevel;
		use log::{Level, RecordBuilder};

		// Coerce fatal down to error since there's no trivial equivalent.
		let log_level = match level {
			TraceLevel::Debug => Level::Debug,
			TraceLevel::Fatal | TraceLevel::Error => Level::Error,
			TraceLevel::Warn => Level::Warn,
			TraceLevel::Info => Level::Info,
			TraceLevel::Trace => Level::Trace,
			// git2 should not produce trace messages with no level.
			other @ TraceLevel::None => panic!("Unsupported git2 log level: {other:?}"),
		};

		let mut record_builder = RecordBuilder::new();

		record_builder.level(log_level).target("libgit2");

		let msg_str = std::str::from_utf8(msg).unwrap_or("non-UTF8 string received in callback");

		log::logger().log(&record_builder.args(format_args!("{}", msg_str)).build());
	})
	.map_err(|e| hc_error!("Failed to set git2 callback: {}", e))
	.unwrap();
}
