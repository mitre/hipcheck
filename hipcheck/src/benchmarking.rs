//! Special tools/utilities for benchmarking.
//!
//! The way this is intended to work is that when the "print-timings" feature is enabled,
//! various lines of code that create instantiate structs like [PrintTime] will be activated.
//!
//! These structs have a special [Drop] implementation that prints the duration between instantiation and when they
//! are dropped, effectively giving us function timings without being too intrusive to the codebase.
//!
//! A macro is provided to automatically tag timings with the file and line number that they were created on.

use std::time::Instant;

/// Structure used to track timing that will print its location and elapsed time when dropped.
#[derive(Debug)]
pub struct PrintTime {
	pub location: String,
	pub start: Instant,
}

impl PrintTime {
	/// Create a new [PrintTime] that will print the number of seconds it was alive for when it gets dropped.
	pub fn new(location: impl Into<String>) -> Self {
		PrintTime {
			location: location.into(),
			start: Instant::now(),
		}
	}
}

impl Drop for PrintTime {
	fn drop(&mut self) {
		Shell::print_timing(&self)
	}
}

macro_rules! print_scope_time {
	() => {
		$crate::benchmarking::PrintTime::new(format!(
			"{}:{}:{}",
			module_path!(),
			line!(),
			column!()
		))
	};

	($msg:literal) => {
		$crate::benchmarking::PrintTime::new(format!(
			"{}:{}:{} ({})",
			module_path!(),
			line!(),
			column!(),
			$msg
		))
	};
}

pub(crate) use print_scope_time;

use crate::shell::Shell;
