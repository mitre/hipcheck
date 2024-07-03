//! A spinner phase is a phase in hipcheck that makes progress without a pre-determined length. 
//! 
//! This can be useful for things like while-loops and iterators without a known size. 

use std::{fmt::Display, sync::{Arc, OnceLock}, time::Duration};
use console::{style, Emoji};
use indicatif::{ProgressBar, ProgressStyle};
use super::{Shell, ERROR_ESCLAMATION, GREEN_CHECKBOX, HOUR_GLASS, ROCKET_SHIP};

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
	pub(super) bar: ProgressBar
}

impl SpinnerPhase {
    /// Create a new spinner progress bar and attach it to the global [Shell]'s multi-progress. 
    /// 
    /// The phase will remain in the "starting..." state until incremented. 
    pub fn start(name: impl Into<Arc<str>>) -> Self {
        // Create the spinner progress bar with our styling. 
        let bar = ProgressBar::new_spinner()
            .with_style(spinner_style().clone());

		let name = name.into();

		// Set the initial message of the bar. 
		bar.set_prefix(ROCKET_SHIP.to_string());
		bar.set_message(format!("{name} (starting...)"));

        // Add to the global shell.
        Shell::progress_bars().add(bar.clone());

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

    /// Internal function to finish with a new status and an emoji.
    fn finish_status(&self, status: impl Display, prefix: &Emoji) {
        self.bar.set_message(format!("{} ({status})", self.name));
        self.bar.set_prefix(prefix.to_string());
        self.bar.finish()
    }

    /// Finishes this spinner, leaving it in the terminal with an updated "done" message and a green check.
    pub fn finish_successful(&self) {
        self.finish_status(style("done").green(), &GREEN_CHECKBOX);
    }

    /// Finish this spinner, leaving it in the terminal with an updated "error" message and a red exclamation. 
    pub fn finish_error(&self) {
        self.finish_status(style("error").red().bold(), &ERROR_ESCLAMATION);
    }
}


/// A spinner phase tracking an [Iterator]. 
pub struct SpinnerPhaseTracker<I> {
    pub phase: SpinnerPhase,
    pub iter: I
}
