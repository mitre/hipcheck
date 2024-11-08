// SPDX-License-Identifier: Apache-2.0

#![deny(missing_docs)]

use crate::{
	cli::Format,
	error::{Error, Result},
	report::{RecommendationKind, Report},
};
use console::{Emoji, Style, Term};
use indicatif::{MultiProgress, ProgressDrawTarget};
use std::{
	fmt,
	fmt::{Alignment, Debug, Display, Formatter},
	io::Write,
	sync::{OnceLock, RwLock},
};
use verbosity::{SilenceGuard, Verbosity};

#[cfg(feature = "print-timings")]
use console::style;

#[cfg(feature = "print-timings")]
use std::time::Instant;

pub mod color_choice;
pub mod iter;
pub mod macros;
pub mod par_iter;
pub mod progress_phase;
pub mod spinner_phase;
pub mod verbosity;

/// Global static shell instance, stored in a [`OnceLock`] to make it thread safe and lazy.
static GLOBAL_SHELL: OnceLock<Shell> = OnceLock::new();

const ROCKET_SHIP: Emoji = Emoji("🚀", "....");
const HOUR_GLASS: Emoji = Emoji("⏳", ">>>>");

/// The width of the left column when printing errors/reports/etc.
pub const LEFT_COL_WIDTH: usize = 20;

/// Empty static string used for drawing padding.
const EMPTY: &str = "";

/// Type interface to the global shell used to produce output in the user's terminal.
#[derive(Debug)]
pub struct Shell {
	/// Multi-progress bar object rendering all the different progress bars we're using.
	multi_progress: MultiProgress,
	/// The verbosity of this shell.
	verbosity: RwLock<Verbosity>,
}

impl Shell {
	/// Initialize the global shell. Panics if the global shell is already initialized.
	pub fn init(verbosity: Verbosity) {
		if GLOBAL_SHELL.get().is_some() {
			panic!("Global shell is already initialized");
		}

		GLOBAL_SHELL.get_or_init(move || Shell {
			multi_progress: MultiProgress::new(),
			verbosity: RwLock::new(verbosity),
		});
	}

	/// Check if the global shell is initialized.
	pub fn is_init() -> bool {
		Shell::try_get().is_some()
	}

	/// Get the global shell if it's initialized or return [None].
	#[inline]
	pub fn try_get() -> Option<&'static Self> {
		GLOBAL_SHELL.get()
	}

	/// Get a static reference to the global shell. Panics if the global shell has not yet been initialized.
	pub fn get() -> &'static Self {
		Self::try_get().expect("global shell needs to be initialized.")
	}

	/// Update the verbosity of the global shell.
	pub fn set_verbosity(verbosity: Verbosity) {
		// If the new verbosity is "silent", hide all progress bars.
		if verbosity == Verbosity::Silent {
			Shell::get()
				.multi_progress
				.set_draw_target(ProgressDrawTarget::hidden());
		}

		let mut write_guard = Self::get()
			.verbosity
			.write()
			.expect("acquired write guard to global verbosity");

		// If moving from a "silent" verbosity state to a not-silent verbosity, reset the
		// draw target for the progress bars back to the stderr (default).
		if *write_guard == Verbosity::Silent && verbosity != Verbosity::Silent {
			Shell::get()
				.multi_progress
				.set_draw_target(ProgressDrawTarget::stderr());
		}

		*write_guard = verbosity;
	}

	/// Silence the global [Shell] for the remainder of this scope.
	///
	/// This is equivalent to using [Shell::set_verbosity] to silence the global shell,
	/// and then reseting it back to the previous value whenever the returned [SilenceGuard] is [drop]ped.
	pub fn silence() -> SilenceGuard {
		let previous_verbosity = Shell::get_verbosity();
		Shell::set_verbosity(Verbosity::Silent);
		SilenceGuard { previous_verbosity }
	}

	/// Get the current verbosity of the global shell.
	///
	/// Be aware that the value may become outdated if another thread calls [Shell::set_verbosity].
	pub fn get_verbosity() -> Verbosity {
		let guard = Self::get()
			.verbosity
			.read()
			.expect("acquired read guard to global verbosity");

		// Deref-copy and return.
		*guard
	}

	/// Update whether colors are enabled for all of hipcheck.
	pub fn set_colors_enabled(enable: bool) {
		console::set_colors_enabled(enable);
		console::set_colors_enabled_stderr(enable);
	}

	/// Get a clone of the [`MultiProgress`] instance stored using [`Arc::clone`] under the hood.
	#[allow(unused)]
	pub fn progress_bars() -> MultiProgress {
		Self::get().multi_progress.clone()
	}

	/// Print timing info. Only enabled while hipcheck is being benchmarked.
	#[allow(unused)]
	#[cfg(feature = "benchmarking")]
	pub fn print_timing(timing: &crate::benchmarking::PrintTime) {
		use crate::benchmarking::PrintTime;

		// Destructure timing object.
		let PrintTime { location, start } = timing;

		Shell::in_suspend(|| {
			eprintln!(
				"[TIMINGS]: {}: {:.6} seconds elapsed.",
				style(location).bold(),
				(Instant::now() - *start).as_secs_f64()
			);
		})
	}

	/// Print a message to the standard output if the standard output is a terminal.
	/// Panics if the global shell is not initialized or if there's an issue printing to the standard output.
	#[allow(unused)]
	pub fn println_if_terminal(msg: impl AsRef<str>) {
		Shell::get()
			.multi_progress
			.println(msg)
			.expect("could print to standard output")
	}

	/// Suspend and hide all progress bars to write to the standard output or standard error.
	/// Do not do heavy coomputation here since the lock on the progress bars is held the whole time and
	/// may cause other threads to block waiting on a lock.
	///
	/// # Panics
	/// - Panics if the global shell is not initialized.
	pub fn in_suspend<F, R>(f: F) -> R
	where
		F: FnOnce() -> R,
	{
		Self::get().multi_progress.suspend(f)
	}

	/// Print a message regardless of whether or not the standard output is a terminal.
	/// [Shell::println_if_terminal] may be more desirable in some cases.
	///
	/// This will temporarily hide the progress bars to print.
	///
	/// # Panics
	/// - Panics if the global logger is not initialized.
	pub fn println(msg: impl Display) {
		// Do not print if verbosity is set to silent.
		if Shell::get_verbosity() == Verbosity::Silent {
			return;
		}

		Shell::in_suspend(|| {
			println!("{msg}");
		})
	}

	/// Bypass the recommended styling and print a message to the standard error.
	/// Temporarily hide the progress bar to print.
	///
	/// # Panics
	/// - Panics if the global logger is not initialized.
	pub fn eprintln(msg: impl Display) {
		Shell::in_suspend(|| {
			eprintln!("{}", msg);
		})
	}

	/// Print "Analysing {source}" with the proper color/styling.
	pub fn print_prelude(source: impl AsRef<str>) {
		match Shell::get_verbosity() {
			Verbosity::Normal => {
				macros::println!("{:>LEFT_COL_WIDTH$} {}", Title::Analyzing, source.as_ref());
			}
			Verbosity::Quiet | Verbosity::Silent => {}
		}
	}

	/// Print a hipcheck [Error]. Human readable errors will go to the standard error, JSON will go to the standard output.
	pub fn print_error(err: &Error, format: Format) {
		match format {
			Format::Human => {
				// Print the root error -- the first in the chain should not be none.
				let mut chain = err.chain();
				macros::eprintln!("{}", chain.next().expect("chain is not empty"));

				// Print remaining errors in chain.
				for err in chain {
					macros::eprintln!("{EMPTY:LEFT_COL_WIDTH$}{}", err);
				}

				// Print an extra newline at the end to separate error printing from other stuff.
				macros::eprintln!();
			}

			Format::Json => {
				// Construct a JSON value from an error.
				let current = err.to_string();
				let context = err
					.chain()
					.skip(1)
					.map(ToString::to_string)
					.collect::<Vec<_>>();

				let error_json = serde_json::json!({
					"Error": {
						"message": current,
						"context": context,
					}
				});

				log::trace!("writing message part [part='{:?}']", error_json);

				// Suspend the progress bars to print the JSON.
				Shell::in_suspend(|| {
					let mut stdout = Term::buffered_stdout();

					serde_json::to_writer_pretty(&mut stdout, &error_json)
						.expect("Wrote JSON to standard output.");

					writeln!(&mut stdout).expect("wrote newline to standard out");
					stdout.flush().expect("flushed standard out");
				});
			}
		}
	}

	/// Print the final repo report in the requested format to the standard output.
	pub fn print_report(report: Report, format: Format) -> Result<()> {
		match format {
			Format::Json => print_json(report),
			Format::Human => print_human(report),
		}
	}
}

fn print_json(report: Report) -> Result<()> {
	// Suspend the shell to print the JSON report.
	Shell::in_suspend(|| {
		let mut stdout = Term::stdout();
		serde_json::to_writer_pretty(&mut stdout, &report)?;
		stdout.flush()?;
		Ok(())
	})
}

fn print_human(report: Report) -> Result<()> {
	// Go through each part and print them individually.

	//      Analyzed '<repo_name>' (<repo_head>)
	//               using Hipcheck <hipcheck_version>
	//               at <analyzed_at:pretty_print>
	//
	//       Passing
	//            + no concerning contributors
	//               0 found, 0 permitted
	//            + no unusually large commits
	//               0% of commits are unusually large, 2% permitted
	//            + commits usually merged by someone other than the author
	//               0% of commits merged by author, 20% permitted
	//
	//       Failing
	//            - too many unusual-looking commits
	//               1% of commits look unusual, 0% permitted
	//               entropy scores over 3.0 are considered unusual
	//
	//              356828346723752 "fixing something" (entropy score: 5.4)
	//              abab563268c3543 "adding a new block" (entropy score: 3.2)
	//
	//           - hasn't been updated in 136 weeks
	//              require updates in the last 71 weeks
	//
	//        Errored
	//           ? review analysis failed to get pull request reviews
	//              cause: missing GitHub token with permissions for accessing public repository data in config
	//           ? typo analysis failed to get dependencies
	//              cause: can't identify a known language in the repository
	//
	// Recommendation
	//           PASS risk rated as 0.4 (acceptable below 0.5)

	/*===============================================================================
	 * Header
	 *
	 * Says what we're analyzing, what version of Hipcheck we're using, and when
	 * we're doing the analysis.
	 */

	// Start with an empty line.
	macros::println!();
	// What repo we analyzed.
	macros::println!("{:>LEFT_COL_WIDTH$} {}", Title::Analyzed, report.analyzed());
	// With what version of hipcheck.
	macros::println!("{EMPTY:LEFT_COL_WIDTH$} {}", report.using());
	// At what time.
	macros::println!("{EMPTY:LEFT_COL_WIDTH$} {}", report.at_time());
	// Space between this and analyses.
	macros::println!();

	/*===============================================================================
	 * Passing analyses
	 *
	 * Says what analyses passed and why.
	 */

	if report.has_passing_analyses() {
		macros::println!("{:>LEFT_COL_WIDTH$}", Title::Section("Passing"));

		for analysis in report.passing_analyses() {
			macros::println!(
				"{:>LEFT_COL_WIDTH$} {}",
				Title::Passed,
				analysis.statement()
			);
			// Empty line at end to space out analyses.
			macros::println!();
		}
	}

	/*===============================================================================
	 * Failing analyses
	 *
	 * Says what analyses failed, why, and what information might be relevant to
	 * check in detail.
	 */

	if report.has_failing_analyses() {
		macros::println!("{:>LEFT_COL_WIDTH$}", Title::Section("Failing"));

		for failing_analysis in report.failing_analyses() {
			let analysis = failing_analysis.analysis();

			macros::println!(
				"{:>LEFT_COL_WIDTH$} {}",
				Title::Failed,
				analysis.statement()
			);

			for concern in failing_analysis.concerns() {
				macros::println!("{EMPTY:LEFT_COL_WIDTH$} {}", concern);
			}

			// Newline at the end for spacing.
			macros::println!();
		}
	}

	/*===============================================================================
	 * Errored analyses
	 *
	 * Says what analyses encountered errors and what those errors were.
	 */

	if report.has_errored_analyses() {
		macros::println!("{:>LEFT_COL_WIDTH$}", Title::Section("Errored"));

		for errored_analysis in report.errored_analyses() {
			macros::println!(
				"{:>LEFT_COL_WIDTH$} {}",
				Title::Errored,
				errored_analysis.top_msg()
			);

			for msg in &errored_analysis.source_msgs() {
				macros::println!("{EMPTY:LEFT_COL_WIDTH$} {msg}");
			}

			// Newline for spacing.
			macros::println!();
		}
	}

	/*===============================================================================
	 * Recommendation
	 *
	 * Says what Hipcheck's final recommendation is for the target of analysis.
	 */

	let recommendation = report.recommendation();

	macros::println!("{:>LEFT_COL_WIDTH$}", Title::Section("Recommendation"));
	macros::println!(
		"{:>LEFT_COL_WIDTH$} {}",
		Title::from(recommendation.kind),
		recommendation.statement()
	);
	// Newline for spacing.
	macros::println!();

	Ok(())
}

/// The "title" of a message; may be accompanied by a timestamp or outcome.
#[derive(Debug)]
#[allow(unused)]
enum Title {
	/// "Analyzing"
	Analyzing,
	/// "Analyzed"
	Analyzed,
	/// The name of the section.
	Section(&'static str),
	/// An analysis passed.
	Passed,
	/// An analysis failed.
	Failed,
	/// An analysis errored out.
	Errored,
	/// "In Progress"
	InProgress,
	/// "Done"
	Done,
	/// "PASS"
	Pass,
	/// "INVESTIGATE"
	Investigate,
	/// "Error"
	Error,
}

impl Title {
	const fn text(&self) -> &str {
		use Title::*;

		match self {
			Analyzing => "Analyzing",
			Analyzed => "Analyzed",
			Section(s) => s,
			Passed => "+",
			Failed => "-",
			Errored => "?",
			InProgress => "In Progress",
			Done => "Done",
			Pass => "PASS",
			Investigate => "INVESTIGATE",
			Error => "Error",
		}
	}

	fn style(&self) -> Style {
		use console::Color::*;
		use Title::*;

		let color = match self {
			Analyzed | Section(..) => Some(Blue),
			Analyzing | Done => Some(Cyan),
			InProgress => Some(Magenta),
			Passed | Pass => Some(Green),
			Failed | Investigate => Some(Red),
			Errored => Some(Yellow),
			Error => Some(Red),
		};

		match color {
			Some(c) => Style::new().fg(c).bold(),
			None => Style::new(),
		}
	}
}

impl Display for Title {
	fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
		match f.width() {
			// No width designation -- just write the styled string itself.
			None => write!(f, "{}", self.style().apply_to(self.text())),

			// Width/alignment/padding handled here.
			Some(width) => {
				let styled: String = self.style().apply_to(self.text()).to_string();

				// Convert the alignment passed to the formatter. If there is no alignment, default to
				// left align.
				let align = f
					.align()
					.map(convert_alignment)
					.unwrap_or(console::Alignment::Left);

				let padded = console::pad_str(&styled, width, align, None);
				f.write_str(&padded)
			}
		}
	}
}

/// Convert an [Alignment] from [std::fmt] to [console::Alignment] trivially, using a match arm.
const fn convert_alignment(align: Alignment) -> console::Alignment {
	match align {
		Alignment::Left => console::Alignment::Left,
		Alignment::Right => console::Alignment::Right,
		Alignment::Center => console::Alignment::Center,
	}
}

impl From<RecommendationKind> for Title {
	fn from(kind: RecommendationKind) -> Title {
		match kind {
			RecommendationKind::Pass => Title::Pass,
			RecommendationKind::Investigate => Title::Investigate,
		}
	}
}
