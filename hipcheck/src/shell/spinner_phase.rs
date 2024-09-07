// SPDX-License-Identifier: Apache-2.0

//! A spinner phase is a phase in hipcheck that makes progress without a pre-determined length.
//!
//! This can be useful for things like while-loops and iterators without a known size.

use super::{verbosity::Verbosity, Shell, HOUR_GLASS, LEFT_COL_WIDTH, ROCKET_SHIP};
use crate::shell::Title;
use console::style;
use indicatif::{HumanDuration, ProgressBar, ProgressDrawTarget, ProgressStyle};
use std::{
	fmt::Display,
	sync::{Arc, OnceLock},
	time::Duration,
};

/// Global static storing the style that should be used for spinners.
static SPINNER_STYLE: OnceLock<ProgressStyle> = OnceLock::new();

/// Get the global static spinner style.
pub fn spinner_style() -> &'static ProgressStyle {
	SPINNER_STYLE.get_or_init(|| {
		ProgressStyle::with_template("{prefix:.bold.dim} {spinner} {wide_msg} {elapsed:>.italic}")
			.expect("valid spinner style")
	})
}

/// A spinner phase (unknown duration/completion length) in the processing of hipcheck.
///
/// This phase will contain and manage a spinner [ProgressBar] that will print its status and will print a
/// completion message when it finishes.
///
/// This struct is cheap to clone, since it's just 4 [Arc]s.
#[derive(Debug, Clone)]
pub struct SpinnerPhase {
	pub(super) name: Arc<str>,
	pub(super) bar: ProgressBar,
}

impl SpinnerPhase {
	/// Create a new spinner progress bar and attach it to the global [Shell]'s multi-progress.
	///
	/// The phase will remain in the "starting..." state until incremented.
	pub fn start(name: impl Into<Arc<str>>) -> Self {
		// Add to the global shell, only if Verbosity::Normal
		let bar = match Shell::get_verbosity() {
			Verbosity::Quiet | Verbosity::Silent => {
				// ProgressBar::new_spinner internally assumes data will be written to stderr, which is not what is wanted for Silent/Quiet
				ProgressBar::with_draw_target(None, ProgressDrawTarget::hidden())
			}
			Verbosity::Normal => {
				let bar = ProgressBar::new_spinner().with_style(spinner_style().clone());
				Shell::progress_bars().add(bar.clone());
				bar
			}
		};

		let name = name.into();

		// Set the initial message of the bar.
		bar.set_prefix(ROCKET_SHIP.to_string());
		bar.set_message(format!("{name} (starting...)"));

		// Return phase object.
		Self { name, bar }
	}

	/// Get the elapsed time since this bar was created.
	pub fn elapsed(&self) -> Duration {
		self.bar.elapsed()
	}

	/// Increment the spinner forward.
	pub fn inc(&self) {
		if self.bar.position() == 0 {
			self.bar.set_message(format!("{} (running...)", self.name));
			self.bar.set_prefix(HOUR_GLASS.to_string());
		}

		self.bar.inc(1)
	}

	/// Update the status and redraw this bar with the new status.
	/// This status may be over-written if the bar changes states into "done" or the status is updated otherwise.
	pub fn update_status(&self, status: impl Display) {
		self.bar.set_message(format!("{} ({status})", self.name));
		self.bar.set_prefix(HOUR_GLASS.to_string());
	}

	/// Set this spinner phase to tick steadily.
	///
	/// It is best practice to call [SpinnerPhase::inc] first to update the bar state to "running...".
	///
	/// This will cause the spinner to rotate indefinitely until finished, which is useful for processes
	/// that don't report meainingful progress info (like external commands).
	pub fn enable_steady_tick(&self, interval: Duration) {
		self.bar.enable_steady_tick(interval);
	}

	/// Finishes this spinner, leaving it in the terminal with an updated "done" message.
	pub fn finish_successful(&self) {
		match Shell::get_verbosity() {
			Verbosity::Normal => {
				super::macros::println!(
					"{:>LEFT_COL_WIDTH$} {} ({})",
					Title::Done,
					self.name,
					style(HumanDuration(self.elapsed())).bold()
				);
			}
			Verbosity::Quiet | Verbosity::Silent => {}
		}
		self.bar.finish_and_clear()
	}

	#[allow(unused)]
	/// Finish this spinner, leaving it in the terminal with an updated "error" message and a red exclamation.
	pub fn finish_error(&self) {
		super::macros::println!(
			"{:>LEFT_COL_WIDTH$} {} ({})",
			Title::Errored,
			self.name,
			style(HumanDuration(self.elapsed())).bold()
		);

		self.bar.finish_and_clear()
	}
}

/// A spinner phase tracking an [Iterator].
pub struct SpinnerPhaseTracker<I> {
	pub phase: SpinnerPhase,
	pub iter: I,
}
