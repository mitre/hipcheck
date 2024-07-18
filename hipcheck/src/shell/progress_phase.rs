//! A progress phase is a phase in hipcheck that makes progress with a known length.
//!
//! This can be useful for things like like file download, where the number of bytes is known.

use crate::shell::Title;

use super::{Shell, HOUR_GLASS, LEFT_COL_WIDTH, ROCKET_SHIP};
use console::style;
use indicatif::{HumanDuration, ProgressBar, ProgressStyle};
use std::{
	fmt::Display,
	sync::{Arc, OnceLock},
	time::Duration,
};

/// Global static storing the style that should be used for progress bars over a number of units.
static STYLES: OnceLock<[ProgressStyle; 2]> = OnceLock::new();

/// Get the global static spinner style.
fn get_styles() -> &'static [ProgressStyle] {
	STYLES.get_or_init(|| [
		// Unit agnostic progress style. 
        ProgressStyle::with_template("{prefix:.bold.dim} {msg} {wide_bar} [{pos}/{len}] ({percent:>3.bold}%) {elapsed:.italic}/{duration:.italic}")
            .expect("valid style"),

		// Bytes/file transfer prgress style. 
		ProgressStyle::with_template("{prefix:.bold.dim} {msg} {wide_bar} \
			[{decimal_bytes}/{decimal_total_bytes}: {decimal_bytes_per_sec:.bold}] ({percent:>3.bold}%) {elapsed:.italic}/{duration:.italic}")
			.expect("valid style"),
    ])
}

/// Get a unit-agnostic/unit-unaware style for drawing progress bars.
#[allow(unused)]
pub fn get_unit_agnostic_style() -> &'static ProgressStyle {
	&get_styles()[0]
}

/// Get a progress bar style that formats the position and length as bytes.
#[allow(unused)]
pub fn get_bytes_style() -> &'static ProgressStyle {
	&get_styles()[1]
}

/// A phase with a progress bar with a known completion status (number of bytes/number of iterations/etc).
///
/// This phase will contain and manage a [ProgressBar] that will print its status and will print a completion message when
/// the phase finishes.
#[derive(Clone, Debug)]
pub struct ProgressPhase {
	pub(super) name: Arc<str>,
	pub(super) bar: ProgressBar,
}

#[allow(unused)]
impl ProgressPhase {
	/// Create a new progress bar and attach it to the global [Shell]'s multi-progress.
	///
	/// The phase will remain in the "starting..." state until incremented.
	///
	/// By default this uses a "unit agnostic" styling for the progress bar.
	/// If you want a progress bar that prints progress as bytes, use [Self::start_bytes].
	pub fn start(len: u64, name: impl Into<Arc<str>>) -> Self {
		// Create the progress bar.
		let bar = ProgressBar::new(len).with_style(get_unit_agnostic_style().clone());

		// Add the progress bar to the shell.
		Shell::progress_bars().add(bar.clone());

		// Convert the name.
		let name = name.into();

		// Set the bar's prefix and message.
		bar.set_message(format!("{name} (starting...)"));
		bar.set_prefix(ROCKET_SHIP.to_string());

		// Return.
		Self { name, bar }
	}

	/// Create a new progress bar and attach it to the global [Shell]'s multi-progress.
	/// This phase will draw with the current position and total length represented as a number of bytes.
	///
	/// The phase will remain in the "starting..." state until incremented.
	pub fn start_bytes(bytes: u64, name: impl Into<Arc<str>>) -> Self {
		let phase = Self::start(bytes, name);
		phase.bar.set_style(get_bytes_style().clone());
		// Force re-draw.
		phase.bar.tick();
		phase
	}

	/// Get the elapsed time since this bar was created.
	pub fn elapsed(&self) -> Duration {
		self.bar.elapsed()
	}

	/// Increment the progress bar by a certain amount of progress.
	pub fn inc(&self, amount: u64) {
		if self.bar.position() == 0 {
			self.bar.set_message(format!("{} (running...)", self.name));
			self.bar.set_prefix(HOUR_GLASS.to_string());
		}

		self.bar.inc(amount)
	}

	/// Set the current amount of progress made.
	pub fn set_position(&self, new_position: u64) {
		if self.bar.position() == 0 && new_position > 0 {
			self.bar.set_message(format!("{} (running...)", self.name));
			self.bar.set_prefix(HOUR_GLASS.to_string());
		}

		self.bar.set_position(new_position);
	}

	/// Update the status and redraw this bar with the new status.
	/// This status may be over-written if the bar changes states into "done" or the status is updated otherwise.
	pub fn update_status(&self, status: impl Display) {
		self.bar.set_message(format!("{} ({status})", self.name));
		self.bar.set_prefix(HOUR_GLASS.to_string());
	}

	/// Finishes this bar, optionally leaving a "done" message with a timestamp in the terminal.
	pub fn finish_successful(&self, print_message: bool) {
		if print_message {
			super::macros::println!(
				"{:>LEFT_COL_WIDTH$} {} ({})",
				Title::Done,
				self.name,
				style(HumanDuration(self.elapsed())).bold()
			);
		}

		self.bar.finish_and_clear();
	}

	/// Finishes this bar, leaving a "errored" message in the terminal with a timestamp.
	#[allow(unused)]
	pub fn finish_error(&self) {
		super::macros::println!(
			"{:>LEFT_COL_WIDTH$} {} ({})",
			Title::Errored,
			self.name,
			style(HumanDuration(self.elapsed())).bold()
		);

		self.bar.finish_and_clear();
	}

	/// Check if this phase is finished.
	pub fn is_finished(&self) -> bool {
		self.bar.is_finished()
	}
}

/// A progress phase tracking an [Iterator].
pub struct ProgressPhaseTracker<I> {
	pub phase: ProgressPhase,
	pub iter: I,
}
