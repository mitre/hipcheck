// SPDX-License-Identifier: Apache-2.0

/*!
 * The interface to print things to the end-user.
 *
 * ## How to use the `Shell`
 *
 * Generally, you'll interact with the [`Shell`] through the `Session` type. The initialization
 * of the [`Shell`] is one of the first things to happen in Hipcheck after parsing, so we can
 * gracefully report errors if they arise.
 *
 * The [`Shell`] is initialized by providing the desired [`Verbosity`], [`ColorChoice`], and
 * [`Encoding`].
 *
 * Printing progress in Hipcheck has three parts: the prelude, the phases, and the result.
 *
 * The prelude is printed with [`Shell::prelude`], which takes the raw string name of the source
 * to be analyzed. This printing happens _before_ source specifiers are resolved into the
 * `Source` type, which is why it's a string here.
 *
 * Throughout Hipcheck's run, you can add new lines of progress to report using the [`Shell::phase`]
 * method, which takes in a string describing the work being done during the phase, and
 * returns a [`Phase`] handle which can be used to provide update messages with the [`Phase::update`]
 * method, and is closed out with the [`Phase::finish`] method.
 *
 * Finally, the final result is printed using the [`Shell::report`] and [`Shell::pr_report`] methods.
 * These methods don't print using the `Shell`'s `output` and `error_output` streams, but rather
 * accept a caller-provided stream to write to. This allows flexibility in sending the final
 * report to a different destination than log messages.
 * These methods take a `&mut Output`, a [`Report`] or [`PrReport`], and a [`Format`].
 *
 * At any point, errors may also be printed using [`Shell::error`], which takes an
 * [`&Error`][hc_common::error:Error] and a [`Format`].
 *
 * ## Why the shell interface?
 *
 * This crate exists for a few reasons:
 *
 * 1. To ensure Hipcheck always prints things out in a consistent format.
 * 2. To make sure information is always printed to the correct stream.
 * 3. To make it so other parts of the system don't have to worry about printing things.
 * 4. To encapsulate handling of things like color, character encoding, output format, verbosity,
 *    stream redirection, and more.
 *
 * ## Why is the shell implementation so complex?
 *
 * This module is one of the more complex ones within Hipcheck. This complexity is present
 * for a few reasons:
 *
 * - The [`Shell`] type needs to take `&self` for its methods to avoid issues when used in
 *   Hipcheck's central `Session` type (because then we'd be mutating `Session`, which is
 *   a no-no).
 * - The nature of the output can vary along multiple dimensions (format, verbosity, TTY
 *   or not), and those all need to be handled correctly.
 * - Shell printing involves some degree of cross-platform code (in our case to ensure we
 *   are clearing lines consistently on all platforms).
 * - Different types of messages should be printed consistently in a visually pleasing
 *   format.
 *
 * To take care of this, we do a few things here:
 *
 * First, [`Shell`] wraps `Cell<Option<ShellInner>>`, where all the important logic is
 * implemented on [`ShellInner`]. The methods on [`Shell`], simply take the inner value out,
 * call the method on it, and then put it back in the [`Cell`]. This lets us keep the
 * user-facing methods taking `&self` instead of `&mut self`.
 *
 * Then, [`Shell`] incorporates a [`Printer`] type to handle the logic of when to print newline
 * or not, when to silence or not, and when to clear a line or not. All three of these are
 * dependent both on the configuration of the [`Printer`] _and_ the nature of the `Message`
 * being printed. Some messages must always be printed (i.e., the final output messages).
 *
 * Also, we sometimes want lines which we can update with progress as a phase of work
 * progresses. This is achieved with the `Phase` type, which provides a simple API (internally
 * calling methods on [`ShellInner`]) to create and then update a line, before ending a phase and
 * moving on to other messages.
 *
 * We also want to make sure [`Message`] values are printed consistently, which we accomplish
 * using a [`Print`] trait which abstracts over the handling of color, bolding, width, and
 * alignment.
 *
 * Finally, we incorporate some platform-specific code to handle clearing the line, which it
 * turns out is more complicated than you may expect!
 */

#![deny(missing_docs)]

use hc_common::{
	error::{Error, Result},
	hc_error, log, serde_json,
};
use hc_report::{Format, PrReport, RecommendationKind, Report};
use std::cell::Cell;
use std::fmt::{self, Debug, Formatter};
use std::io::stderr;
use std::io::stdout;
use std::io::IsTerminal as _;
use std::io::Write;
use std::ops::Not as _;
use std::str::FromStr;
use std::time::{Duration, Instant};
use termcolor::Color::*;
use termcolor::{self, Color, ColorSpec, NoColor, StandardStream, WriteColor};

/// The interface used throughout Hipcheck to print things out to the user.
pub struct Shell {
	/// A cell wrapper of the inner workings of `Shell`, to enable the
	/// methods on `Shell` to take it by immutable reference.
	inner: Cell<Option<ShellInner>>,
}

/// A convenience macro to generate methods on `Shell` which just delegate to the
/// real implementation on `ShellInner`.
macro_rules! inner_methods {
	( $( $(#[$doc:meta])* $v:vis fn $name:ident($( $param:ident: $type:ty ),*) )* ) => {
		$(
			inner_methods! { @single
				$( #[$doc] )*
				$v fn $name($( $param: $type ),*)
			}
		)*
	};

	( @single $(#[$doc:meta])* $v:vis fn $name:ident($( $param:ident: $type:ty ),*) ) => {
		$(#[$doc])*
		$v fn $name(&self, $($param: $type),*) -> Result<()> {
			let mut inner = self
				.inner
				.take()
				.ok_or_else(|| hc_error!("no writer found"))?;
			inner.$name($($param),*)?;
			self.inner.set(Some(inner));
			Ok(())
		}
	};
}

impl Shell {
	/// Create a new Shell wrapping the output and error streams.
	pub fn new(output: Output, error_output: Output, verbosity: Verbosity) -> Shell {
		let inner = Cell::new(Some(ShellInner::new(output, error_output, verbosity)));
		Shell { inner }
	}

	/// Enter a new phase.
	pub fn phase<'sec, 'desc>(&'sec self, desc: &'desc str) -> Result<Phase<'sec, 'desc>> {
		Phase::new(self, desc)
	}

	inner_methods! {
		/*=======================================================================================
		 * Used externally.
		 */

		/// Print the prelude header for Hipcheck's run.
		pub fn prelude(source: &str)

		/// Print a warning to the user.
		pub fn warn(message: &str)

		/// Print an error.
		pub fn error(error: &Error, format: Format)

		/// Print the final repo report in the requested format.
		pub fn report(output: &mut Output, report: Report, format: Format)

		/// Print the final pull request report in the requested format.
		pub fn pr_report(output: &mut Output, report: PrReport, format: Format)


		/*=======================================================================================
		 * Used internally, by `Phase`.
		 */

		/// Print the in-progress line for a phase.
		fn status(msg: &str)

		/// Print a warning during a phase.
		fn warn_in_phase(msg: &str)

		/// Update the in-progress line for a phase.
		fn update_status(msg: &str)

		/// Finish or update a phase, possibly with a timestamp.
		fn finish_status(msg: &str, elapsed: Option<Duration>)
	}
}

impl Debug for Shell {
	fn fmt(&self, f: &mut Formatter) -> fmt::Result {
		match self.inner.take() {
			Some(inner) => {
				f.debug_struct("Shell").field("inner", &inner).finish()?;

				self.inner.set(Some(inner));
			}
			None => {
				f.debug_struct("Shell").finish()?;
			}
		}

		Ok(())
	}
}

/// The inner contents of a `Shell`.
#[derive(Debug)]
struct ShellInner {
	/// Handle for the output stream.
	output: Output,
	/// Handle for the error output stream.
	error_output: Output,
	/// Interface for printing to the stream.
	printer: Printer,
}

/// Print output data (currently, the final report) to a specific writable
/// stream.
macro_rules! out {
	// Multi-message format.
	// This pattern must be first, as it is more specific than the single
	// message format. A brace-delimited block will be parsed as an
	// expression, but with type `()` if the last statement has a
	// semicolon.
	($self:ident, $output:expr, { $( $e:expr; )* }) => {
		$( out!($self, $output, $e) );*
	};

	// Single-message format.
	// This pattern must be after the previous pattern, so that it does
	// not match the case where there are multiple expressions in a block.
	($self:ident, $output:expr, $msg:expr) => {
		$self.printer.print($output, $msg)?
	};
}

/// Print a message out to the user.
macro_rules! pm {
	// Single-message format.
	($self:ident, $msg:expr) => {
		$self.printer.print(&mut $self.output.out, $msg)?
	};

	// Single-message format (to stderr).
	(@err $self:ident, $msg:expr) => {
		$self.printer.print(&mut $self.error_output.out, $msg)?
	};

	// Update a prior message.
	(@update $self:ident, $msg:expr) => {
		$self.printer.update(&mut $self.output.out, $msg)?
	};

	// Multi-message format.
	($self:ident { $( $e:expr; )* }) => {
		$( pm!($self, $e) );*
	}
}

/// Construct a `Message` with a shorthand format.
///
/// Constructors defined here look like `#name [parameter_name: parameter] thing_to_print`.
macro_rules! m {
	(#title_analyzed $msg:expr) => {
		Message::ReportBasic {
			title: Title::Analyzed,
			msg: $msg,
		}
	};

	(#nothing) => {
		Message::Nothing
	};

	(#more $msg:expr) => {
		Message::ReportBasic {
			title: Title::Empty,
			msg: $msg,
		}
	};

	(#report_nothing) => {
		Message::ReportNothing
	};

	(#report_json $report:expr) => {
		Message::ReportJson { report: $report }
	};

	(#pr_report_json $pr_report:expr) => {
		Message::PrReportJson {
			pr_report: $pr_report,
		}
	};

	(#title_passing) => {
		Message::ReportSection {
			title: Title::Section("Passing"),
		}
	};

	(#title_failing) => {
		Message::ReportSection {
			title: Title::Section("Failing"),
		}
	};

	(#title_errored) => {
		Message::ReportSection {
			title: Title::Section("Errored"),
		}
	};

	(#title_recommendation) => {
		Message::ReportSection {
			title: Title::Section("Recommendation"),
		}
	};

	(#analysis_passed [encoded_as: $encoding:expr] $msg:expr) => {
		Message::ReportBasic {
			title: Title::Passed,
			msg: $msg,
		}
	};

	(#analysis_failed [encoded_as: $encoding:expr] $msg:expr) => {
		Message::ReportBasic {
			title: Title::Failed,
			msg: $msg,
		}
	};

	(#analysis_errored $msg:expr) => {
		Message::ReportBasic {
			title: Title::Errored,
			msg: $msg,
		}
	};

	(#analysis_explanation $msg:expr) => {
		Message::ReportBasic {
			title: Title::Empty,
			msg: $msg,
		}
	};

	(#recommendation [kind: $kind:expr] $msg:expr) => {
		Message::ReportBasic {
			title: Title::from($kind),
			msg: $msg,
		}
	};

	(#finish [elapsed: $duration:expr] $msg:expr) => {
		match $duration {
			Some(duration) => Message::Timestamped {
				timestamp: Timestamp(duration),
				title: Title::TimestampedDone,
				msg: $msg,
			},
			None => Message::Basic {
				title: Title::Done,
				msg: $msg,
			},
		}
	};

	(#in_progress $msg:expr) => {
		Message::Basic {
			title: Title::InProgress,
			msg: $msg,
		}
	};

	(#warning $msg:expr) => {
		Message::Warning {
			title: Title::Warning,
			msg: $msg,
		}
	};

	(#phase_warning $msg:expr) => {
		Message::PhaseWarning {
			title: Title::Warning,
			msg: $msg,
		}
	};

	(#error $error:expr) => {
		Message::Error {
			title: Title::Error,
			error: $error,
		}
	};

	(#error_json $error:expr) => {
		Message::ErrorJson { error: $error }
	};

	(#prelude $msg:expr) => {
		Message::Prelude {
			title: Title::Analyzing,
			msg: $msg,
		}
	};
}

impl ShellInner {
	/// Create a new `ShellInner` wrapping the output and error streams.
	fn new(output: Output, error_output: Output, verbosity: Verbosity) -> ShellInner {
		let printer = Printer::new(output.is_atty, verbosity);
		let shell = ShellInner {
			output,
			error_output,
			printer,
		};

		log::debug!("created shell [shell='{:?}']", shell);

		shell
	}

	/// Print the prelude header for Hipcheck's run.
	fn prelude(&mut self, source: &str) -> Result<()> {
		Ok(pm!(@update self, m!(#prelude source)))
	}

	/// Print a warning to the user.
	fn warn(&mut self, msg: &str) -> Result<()> {
		Ok(pm!(self, m!(#warning msg)))
	}

	fn warn_in_phase(&mut self, msg: &str) -> Result<()> {
		Ok(pm!(self, m!(#phase_warning msg)))
	}

	/// Print an error.
	fn error(&mut self, error: &Error, format: Format) -> Result<()> {
		match format {
			Format::Json => Ok(pm!(@err self, m!(#error_json error))),
			Format::Human => Ok(pm!(@err self, m!(#error error))),
		}
	}

	/// Print the in-progress line for a phase.
	fn status(&mut self, msg: &str) -> Result<()> {
		Ok(pm!(self, m!(#in_progress msg)))
	}

	/// Update the in-progress line for a phase.
	fn update_status(&mut self, msg: &str) -> Result<()> {
		Ok(pm!(@update self, m!(#in_progress msg)))
	}

	/// Finish or update a phase, possibly with a timestamp.
	fn finish_status(&mut self, msg: &str, elapsed: Option<Duration>) -> Result<()> {
		Ok(pm!(@update self, m!(#finish [elapsed: elapsed] msg)))
	}

	/// Print the final repo report in the requested format.
	/// Instead of printing to the Shell's own output or error_output,
	/// this method takes a caller-provided Output to print to.
	fn report(&mut self, output: &mut Output, report: Report, format: Format) -> Result<()> {
		match format {
			Format::Json => Ok(out!(self, &mut output.out, m!(#report_json report))),
			Format::Human => {
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

				out!(self, &mut output.out, {
					m!(#nothing);
					m!(#title_analyzed &report.analyzed());
					m!(#more &report.using());
					m!(#more &report.at_time());
					m!(#report_nothing);
				});

				/*===============================================================================
				 * Passing analyses
				 *
				 * Says what analyses passed and why.
				 */

				if report.has_passing_analyses() {
					out!(self, &mut output.out, m!(#title_passing));

					for analysis in report.passing_analyses() {
						out!(self, &mut output.out, {
							m!(#analysis_passed [encoded_as: self.encoding] &analysis.statement());
							m!(#analysis_explanation &analysis.explanation());
							m!(#report_nothing);
						});
					}
				}

				/*===============================================================================
				 * Failing analyses
				 *
				 * Says what analyses failed, why, and what information might be relevant to
				 * check in detail.
				 */

				if report.has_failing_analyses() {
					out!(self, &mut output.out, m!(#title_failing));

					for failing_analysis in report.failing_analyses() {
						let analysis = failing_analysis.analysis();

						out!(self, &mut output.out, {
							m!(#analysis_failed [encoded_as: self.encoding] &analysis.statement());
							m!(#analysis_explanation &analysis.explanation());
						});

						for concern in failing_analysis.concerns() {
							out!(self, &mut output.out, m!(#more &concern.description()));
						}

						out!(self, &mut output.out, m!(#report_nothing));
					}
				}

				/*===============================================================================
				 * Errored analyses
				 *
				 * Says what analyses encountered errors and what those errors were.
				 */

				if report.has_errored_analyses() {
					out!(self, &mut output.out, m!(#title_errored));

					for errored_analysis in report.errored_analyses() {
						out!(
							self,
							&mut output.out,
							m!(#analysis_errored &errored_analysis.top_msg())
						);

						for msg in &errored_analysis.source_msgs() {
							out!(self, &mut output.out, m!(#more msg));
						}

						out!(self, &mut output.out, m!(#report_nothing));
					}
				}

				/*===============================================================================
				 * Recommendation
				 *
				 * Says what Hipcheck's final recommendation is for the target of analysis.
				 */

				let recommendation = report.recommendation();

				out!(self, &mut output.out, {
					m!(#title_recommendation);
					m!(#recommendation [kind: recommendation.kind] &recommendation.statement());
					m!(#report_nothing);
				});

				Ok(())
			}
		}
	}

	/// Print the final pull request report in the requested format.
	/// Instead of printing to the Shell's own output or error_output,
	/// this method takes a caller-provided Output to print to.
	fn pr_report(&mut self, output: &mut Output, report: PrReport, format: Format) -> Result<()> {
		match format {
			Format::Json => Ok(out!(self, &mut output.out, m!(#pr_report_json report))),
			Format::Human => {
				//      Analyzed '<pr_uri>' (<repo_head>)
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

				out!(self, &mut output.out, {
					m!(#nothing);
					m!(#title_analyzed &report.analyzed());
					m!(#more &report.using());
					m!(#more &report.at_time());
					m!(#report_nothing);
				});

				/*===============================================================================
				 * Passing analyses
				 *
				 * Says what analyses passed and why.
				 */

				if report.has_passing_analyses() {
					out!(self, &mut output.out, m!(#title_passing));

					for analysis in report.passing_analyses() {
						out!(self, &mut output.out, {
							m!(#analysis_passed [encoded_as: self.encoding] &analysis.statement());
							m!(#analysis_explanation &analysis.explanation());
							m!(#report_nothing);
						});
					}
				}

				/*===============================================================================
				 * Failing analyses
				 *
				 * Says what analyses failed, why, and what information might be relevant to
				 * check in detail.
				 */

				if report.has_failing_analyses() {
					out!(self, &mut output.out, m!(#title_failing));

					for failing_analysis in report.failing_analyses() {
						let analysis = failing_analysis.analysis();

						out!(self, &mut output.out, {
							m!(#analysis_failed [encoded_as: self.encoding] &analysis.statement());
							m!(#analysis_explanation &analysis.explanation());
						});

						for concern in failing_analysis.concerns() {
							out!(self, &mut output.out, m!(#more &concern.description()));
						}

						out!(self, &mut output.out, m!(#report_nothing));
					}
				}

				/*===============================================================================
				 * Errored analyses
				 *
				 * Says what analyses encountered errors and what those errors were.
				 */

				if report.has_errored_analyses() {
					out!(self, &mut output.out, m!(#title_errored));

					for errored_analysis in report.errored_analyses() {
						out!(
							self,
							&mut output.out,
							m!(#analysis_errored &errored_analysis.top_msg())
						);

						for msg in &errored_analysis.source_msgs() {
							out!(self, &mut output.out, m!(#more msg));
						}

						out!(self, &mut output.out, m!(#report_nothing));
					}
				}

				/*===============================================================================
				 * Recommendation
				 *
				 * Says what Hipcheck's final recommendation is for the target of analysis.
				 */

				let recommendation = report.recommendation();

				out!(self, &mut output.out, {
					m!(#title_recommendation);
					m!(#recommendation [kind: recommendation.kind] &recommendation.statement());
					m!(#report_nothing);
				});

				Ok(())
			}
		}
	}
}

/// A handle for outputting progress in a single phase of work.
pub struct Phase<'sec, 'desc> {
	/// A pointer to the shell.
	shell: &'sec Shell,
	/// The description for the phase.
	desc: &'desc str,
	/// When the phase started.
	started_at: Instant,
}

impl<'sec, 'desc> Phase<'sec, 'desc> {
	/// Construct a new phase, outputting the starting description for it.
	fn new(shell: &'sec Shell, desc: &'desc str) -> Result<Phase<'sec, 'desc>> {
		let started_at = Instant::now();

		shell.status(desc)?;

		Ok(Phase {
			shell,
			desc,
			started_at,
		})
	}

	/// Warn the user.
	pub fn warn(&mut self, msg: &str) -> Result<()> {
		self.shell.warn_in_phase(msg)
	}

	/// Update the phase status with a progress indicator.
	pub fn update(&mut self, progress: &str) -> Result<()> {
		self.shell
			.update_status(&format!("{} ({})", self.desc, progress))
	}

	/// Finish the phase, consuming it and showing the work is done.
	pub fn finish(self) -> Result<()> {
		let ended_at = Instant::now();
		let elapsed = ended_at.duration_since(self.started_at);

		if elapsed.as_millis() == 0 {
			self.shell.finish_status(self.desc, None)
		} else {
			self.shell.finish_status(self.desc, Some(elapsed))
		}
	}
}

/// Wraps the output stream to stdout, stderr, or an arbitrary Write,
/// ensuring proper color behavior.
pub struct Output {
	/// The output stream.
	out: Box<dyn WriteColor>,
	/// Whether the stream is pointing to a TTY.
	is_atty: bool,
}

impl Output {
	/// Create a new Output wrapping stdout.
	pub fn stdout(color_choice: ColorChoice) -> Output {
		let is_atty = stdout().is_terminal();
		let color_choice = color_choice.to_termcolor(is_atty);
		Output {
			out: Box::new(StandardStream::stdout(color_choice)),
			is_atty,
		}
	}

	/// Create a new Output wrapping stderr.
	pub fn stderr(color_choice: ColorChoice) -> Output {
		let is_atty = stderr().is_terminal();
		let color_choice = color_choice.to_termcolor(is_atty);
		Output {
			out: Box::new(StandardStream::stderr(color_choice)),
			is_atty,
		}
	}

	/// Create a new Output wrapping an arbitrary Write.
	pub fn from_writer(write: impl Write + 'static) -> Output {
		Output {
			out: Box::new(NoColor::new(write)),
			is_atty: false,
		}
	}
}

impl Debug for Output {
	fn fmt(&self, f: &mut Formatter) -> fmt::Result {
		f.debug_struct("Output")
			.field("out", &"dyn WriteColor")
			.field("is_atty", &self.is_atty)
			.finish()
	}
}

/// An interface for printing to the shell.
///
/// Provides an interface for printing to the shell that ensures
/// proper line-clearing, verbosity, and newline behavior, depending
/// on both the user configuration and whether output is being sent
/// to a TTY.
#[derive(Debug)]
struct Printer {
	/// The configured verbosity of the output.
	verbosity: Verbosity,
	/// The mode for printing, based on whether output is a TTY.
	mode: PrintMode,
}

impl Printer {
	/// Construct a `Printer` with the appropriate config.
	fn new(is_atty: bool, verbosity: Verbosity) -> Printer {
		let mode = PrintMode::if_out(is_atty);

		Printer { verbosity, mode }
	}

	/// Print to the stream based on configuration without clearing before the next line.
	fn print(&mut self, stream: &mut dyn WriteColor, msg: Message) -> Result<()> {
		self.print_with_clear(stream, Clear::No, msg)
	}

	/// Print to the stream based on configuration and clear before the next line.
	fn update(&mut self, stream: &mut dyn WriteColor, msg: Message) -> Result<()> {
		self.print_with_clear(stream, Clear::Yes, msg)
	}

	/// Print to the stream based on configuration.
	///
	/// This is the core of the printing logic, that ensures the following:
	///
	/// 1. Nothing optional is output in quiet mode.
	/// 2. Lines are cleared when necessary if the configuration supports it.
	/// 3. Newlines are printed when necessary or as required by the configuration.
	///
	/// This function is why the `Printer` type exists.
	fn print_with_clear(
		&mut self,
		stream: &mut dyn WriteColor,
		clear: Clear,
		msg: Message,
	) -> Result<()> {
		// Never print anything in silent mode.
		if self.is_silent() {
			return Ok(());
		}

		// If we're quiet, never print anything.
		// If we need to always print it, then don't skip.
		if self.is_quiet() && msg.may_not_print() {
			return Ok(());
		}

		// If mode supports clearing, and we're set for a clear, erase
		// the current line.
		if self.supports_clear() && clear.should() {
			print(stream, Message::Clear)?;
		}

		// If we don't need to clear the next line, or we need to always
		// print newlines, print a newline.
		if self.requires_newlines() || clear.should().not() {
			print(stream, Message::Newline)?;
		}

		// Print the message.
		print(stream, msg)
	}

	/// Indicates if a newline should be printed after each message.
	fn requires_newlines(&self) -> bool {
		self.mode == PrintMode::Newline
	}

	/// Indicates if the prior line needs to be cleared before printing.
	fn supports_clear(&self) -> bool {
		self.mode == PrintMode::Replace
	}

	/// Indicates if we're in quiet mode.
	fn is_quiet(&self) -> bool {
		self.verbosity == Verbosity::Quiet
	}

	/// Indicates if we're in silent mode.
	fn is_silent(&self) -> bool {
		self.verbosity == Verbosity::Silent
	}
}

/// Indicates whether to clear the line before printing the next.
enum Clear {
	/// Clear the line before printing the next.
	Yes,
	/// Do not clear the line before printing the next.
	No,
}

impl Clear {
	/// Indicates whether it's time to clear the line before printing.
	fn should(&self) -> bool {
		match self {
			Clear::Yes => true,
			Clear::No => false,
		}
	}
}

/// Differentiates printing behavior if the output is a TTY.
#[derive(Debug, PartialEq)]
enum PrintMode {
	/// Only use newlines (non-TTY output)
	Newline,
	/// Sometimes replace a previous line with a new one
	Replace,
}

impl PrintMode {
	/// Sets the print mode based on whether the output stream is a TTY.
	fn if_out(is_atty: bool) -> PrintMode {
		if is_atty {
			PrintMode::Replace
		} else {
			PrintMode::Newline
		}
	}
}

/// How verbose CLI output should be.
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum Verbosity {
	/// Don't output anything except results.
	Quiet,
	/// Output the normal amount of things.
	Normal,
	// This one is only used in testing.
	/// Don't output anything, including results.
	Silent,
}

impl Verbosity {
	/// Check if verbosity is quiet.
	pub fn is_quiet(&self) -> bool {
		matches!(self, Verbosity::Quiet)
	}

	/// Check if verbosity is normal.
	pub fn is_normal(&self) -> bool {
		matches!(self, Verbosity::Normal)
	}
}

impl From<bool> for Verbosity {
	fn from(b: bool) -> Verbosity {
		if b {
			Verbosity::Quiet
		} else {
			Verbosity::Normal
		}
	}
}

/// Selection of whether the CLI output should use color.
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum ColorChoice {
	/// Always use color output
	Always,
	/// Never use color output
	Never,
	/// Guess whether to use color output
	Auto,
}

impl FromStr for ColorChoice {
	type Err = Error;

	fn from_str(s: &str) -> Result<Self> {
		match s.to_lowercase().as_ref() {
			"always" => Ok(ColorChoice::Always),
			"never" => Ok(ColorChoice::Never),
			"auto" => Ok(ColorChoice::Auto),
			_ => Err(Error::msg("unknown color option")),
		}
	}
}

impl ColorChoice {
	/// Convert Color into the `termcolor` crate's equivalent type.
	fn to_termcolor(self, is_atty: bool) -> termcolor::ColorChoice {
		use termcolor::ColorChoice as TermColor;
		use ColorChoice::*;

		match self {
			Always => TermColor::Always,
			Never => TermColor::Never,
			Auto => {
				if is_atty {
					TermColor::Auto
				} else {
					TermColor::Never
				}
			}
		}
	}
}

/// Print a message out to a stream.
fn print(stream: &mut dyn WriteColor, msg: Message) -> Result<()> {
	log::debug!("printing message [message='{:?}']", msg);

	stream.reset()?;

	match msg {
		Message::Prelude { title, msg } => {
			Print::print(&title, stream)?;
			print_str(msg, stream)
		}
		Message::Basic { title, msg } => {
			Print::print(&title, stream)?;
			print_str(msg, stream)
		}
		Message::Timestamped {
			timestamp,
			title,
			msg,
		} => {
			Print::print(&timestamp, stream)?;
			Print::print(&title, stream)?;
			print_str(msg, stream)
		}
		Message::PhaseWarning { title, msg } => {
			Print::print(&title, stream)?;
			print_str(msg, stream)?;
			// Ensure the next phase message doesn't overwrite the warning.
			print_newline(stream)
		}
		Message::Warning { title, msg } => {
			Print::print(&title, stream)?;
			print_str(msg, stream)
			// No newline for regular warnings.
		}
		Message::ErrorJson { error } => print_error_json(error, stream),
		Message::Error { title, error } => {
			Print::print(&title, stream)?;
			print_error(error, stream)
		}
		Message::ReportJson { report } => print_report_json(report, stream),
		Message::PrReportJson { pr_report } => print_pr_report_json(pr_report, stream),
		Message::ReportSection { title } => Print::print(&title, stream),
		Message::ReportBasic { title, msg } => {
			Print::print(&title, stream)?;
			print_str(msg, stream)
		}
		Message::ReportNothing => print_nothing(stream),
		Message::Nothing => print_nothing(stream),
		Message::Newline => print_newline(stream),
		Message::Clear => print_clear(stream),
	}
}

/// Print a string to the stream.
fn print_str(msg: &str, stream: &mut dyn WriteColor) -> Result<()> {
	stream.reset()?;
	let to_write = format!(" {}\r", msg);
	log::trace!("writing message part [part='{:?}']", to_write);
	write!(stream, "{}", to_write)?;
	stream.flush()?;
	Ok(())
}

/// Print an error to the stream.
fn print_error(error: &Error, stream: &mut dyn WriteColor) -> Result<()> {
	log::trace!("printing error");

	stream.reset()?;

	let mut chain = error.chain();

	// PANIC: First error is guaranteed to be present.
	print_str(&chain.next().unwrap().to_string(), stream)?;
	print_newline(stream)?;

	for error in chain {
		log::trace!("printing error in chain [error: {}]", error);

		print_error_source(error, stream)?;
		print_newline(stream)?;
	}

	print_newline(stream)?;

	Ok(())
}

/// Print a source error to the stream.
fn print_error_source(error: &dyn std::error::Error, stream: &mut dyn WriteColor) -> Result<()> {
	stream.reset()?;

	let to_write = format!(" {:>width$}{}", "", error, width = MAX_TITLE_WIDTH);

	log::trace!("writing message part [part='{:?}']", to_write);
	write!(stream, "{}", to_write)?;

	stream.flush()?;

	Ok(())
}

/// Print an error report as JSON.
fn print_error_json(error: &Error, stream: &mut dyn WriteColor) -> Result<()> {
	// Construct a JSON value from an error.
	fn error_to_json(error: &Error) -> serde_json::Value {
		let current = error.to_string();
		let context = error
			.chain()
			.skip(1)
			.map(ToString::to_string)
			.collect::<Vec<_>>();

		serde_json::json!({
			"Error": {
				"message": current,
				"context": context,
			}
		})
	}

	stream.reset()?;
	let error_json = error_to_json(error);
	log::trace!("writing message part [part='{:?}']", error_json);
	serde_json::to_writer_pretty(&mut *stream, &error_json)?;
	writeln!(stream)?;
	stream.flush()?;
	Ok(())
}

/// Print (prettily) a JSON value to the stream for a repo.
fn print_report_json(report: Report, stream: &mut dyn WriteColor) -> Result<()> {
	stream.reset()?;
	log::trace!("writing message part [part='{:?}']", report);
	serde_json::to_writer_pretty(&mut *stream, &report)?;
	stream.flush()?;
	Ok(())
}

/// Print (prettily) a JSON value to the stream for pull requests.
fn print_pr_report_json(pr_report: PrReport, stream: &mut dyn WriteColor) -> Result<()> {
	stream.reset()?;
	log::trace!("writing message part [part='{:?}']", pr_report);
	serde_json::to_writer_pretty(&mut *stream, &pr_report)?;
	stream.flush()?;
	Ok(())
}

/// Print a newline to the stream.
fn print_newline(stream: &mut dyn WriteColor) -> Result<()> {
	log::trace!("writing message part [part='\"\\n\"']");
	writeln!(stream)?;
	stream.flush()?;
	Ok(())
}

/// Print nothing to the stream.
fn print_nothing(stream: &mut dyn WriteColor) -> Result<()> {
	log::trace!("writing message part [part='']");
	write!(stream, "")?;
	stream.flush()?;
	Ok(())
}

/// Print a platform-specific line-clearing output.
fn print_clear(stream: &mut dyn WriteColor) -> Result<()> {
	imp::erase_line(stream)?;
	Ok(())
}

/// Enum representing the possible outputs to the shell.
#[derive(Debug)]
enum Message<'a, 'b> {
	/// Printed at the beginning of Hipcheck's run to say what's being analyzed.
	Prelude { title: Title<'a>, msg: &'a str },
	/// A message during Hipcheck's execution to show progress.
	Basic { title: Title<'a>, msg: &'a str },
	/// A timestamped message when a phase of Hipcheck's run has finished and has
	/// a timestamp.
	Timestamped {
		timestamp: Timestamp,
		title: Title<'a>,
		msg: &'a str,
	},
	/// A warning to the user during a phase.
	PhaseWarning { title: Title<'a>, msg: &'a str },
	/// A warning to the user.
	Warning { title: Title<'a>, msg: &'a str },
	/// An error occured and can't be recovered from, and we're print it as JSON.
	ErrorJson { error: &'b Error },
	/// An error occured and can't be recovered from.
	Error { title: Title<'a>, error: &'b Error },
	/// A JSON version of a final report for a repo.
	ReportJson { report: Report },
	/// A JSON version of a final report for a pull request.
	PrReportJson { pr_report: PrReport },
	/// The title of a section, no content.
	ReportSection { title: Title<'a> },
	/// A regular message in a report.
	ReportBasic { title: Title<'a>, msg: &'a str },
	/// A privileged version of report that doesn't get quieted.
	ReportNothing,
	/// Nothing, plus a newline.
	Nothing,
	/// An empty line.
	Newline,
	/// Clear the current line.
	Clear,
}

impl<'a, 'b> Message<'a, 'b> {
	/// Indicate types of messages which should _always_ be printed.
	fn always_print(&self) -> bool {
		use Message::*;
		matches!(
			self,
			ReportJson { .. }
				| ReportSection { .. }
				| ReportBasic { .. }
				| ReportNothing { .. }
				| Error { .. } | ErrorJson { .. }
		)
	}

	/// Indicate messages that might not print.
	fn may_not_print(&self) -> bool {
		self.always_print().not()
	}
}

/// The "title" of a message; may be accompanied by a timestamp or outcome.
#[derive(Debug)]
enum Title<'a> {
	/// "Analyzing"
	Analyzing,
	/// "Analyzed"
	Analyzed,
	/// The name of the section.
	Section(&'a str),
	/// An analysis passed.
	Passed,
	/// An analysis failed.
	Failed,
	/// An analysis errored out.
	Errored,
	/// "In Progress"
	InProgress,
	/// "Warning"
	Warning,
	/// "Done"
	Done,
	/// "Done" (with a timestamp attached)
	TimestampedDone,
	/// "PASS"
	Pass,
	/// "INVESTIGATE"
	Investigate,
	/// "Error"
	Error,
	/// No title, used when you need the spacing but no text.
	Empty,
}

impl<'a> Print for Title<'a> {
	fn text(&self) -> String {
		use Title::*;

		let text = match self {
			Analyzing => "Analyzing",
			Analyzed => "Analyzed",
			Section(s) => s,
			Passed => "+",
			Failed => "-",
			Errored => "?",
			InProgress => "In Progress",
			Done | TimestampedDone => "Done",
			Pass => "PASS",
			Investigate => "INVESTIGATE",
			Warning => "Warning",
			Error => "Error",
			Empty => "",
		};

		text.to_string()
	}

	fn width(&self) -> usize {
		use Title::*;

		match self {
			// Full width
			Analyzing | Analyzed | Section(..) | Passed | Failed | Errored | InProgress | Done
			| Pass | Investigate | Warning | Error | Empty => MAX_TITLE_WIDTH,
			// Leave room for the timestamp.
			TimestampedDone => DONE_WIDTH,
		}
	}

	fn color(&self) -> Option<Color> {
		use Title::*;

		match self {
			Analyzed | Section(..) => Some(Blue),
			Analyzing | Done | TimestampedDone => Some(Cyan),
			InProgress => Some(Magenta),
			Passed | Pass => Some(Green),
			Failed | Investigate => Some(Red),
			Warning | Errored => Some(Yellow),
			Error => Some(Red),
			Empty => None,
		}
	}

	fn is_bold(&self) -> bool {
		true
	}
}

impl<'a> From<RecommendationKind> for Title<'a> {
	fn from(kind: RecommendationKind) -> Title<'a> {
		match kind {
			RecommendationKind::Pass => Title::Pass,
			RecommendationKind::Investigate => Title::Investigate,
		}
	}
}

/// The outcome of an analysis.
///
/// For space reasons, skip and error are compressed into skip. This also
/// makes it clearer that errors don't factor into calculations.
#[derive(Debug)]
enum Outcome {
	/// Analysis passed.
	Pass,
	/// Analysis failed.
	Fail,
	/// Analysis skipped or errored.
	Skip,
}

impl FromStr for Outcome {
	type Err = Error;

	fn from_str(s: &str) -> Result<Self> {
		match s.to_lowercase().as_ref() {
			"pass" => Ok(Outcome::Pass),
			"fail" => Ok(Outcome::Fail),
			"skip" | "error" => Ok(Outcome::Skip),
			_ => Err(Error::msg("unknown outcome")),
		}
	}
}

impl Print for Outcome {
	fn text(&self) -> String {
		let text = match self {
			Outcome::Pass => "Pass",
			Outcome::Fail => "Fail",
			Outcome::Skip => "Skip",
		};

		text.to_owned()
	}

	fn width(&self) -> usize {
		OUTCOME_WIDTH
	}

	fn color(&self) -> Option<Color> {
		match self {
			Outcome::Pass => Some(Green),
			Outcome::Fail => Some(Red),
			Outcome::Skip => Some(Yellow),
		}
	}

	fn is_bold(&self) -> bool {
		true
	}
}

/// A timestamp for how long a phase took.
#[derive(Debug)]
struct Timestamp(Duration);

impl Print for Timestamp {
	fn text(&self) -> String {
		let secs = self.0.as_secs();
		let millis = self.0.subsec_millis();
		format!("{}.{:0>3.3}s", secs, millis)
	}

	fn width(&self) -> usize {
		// Leave room for the title.
		MAX_TITLE_WIDTH - DONE_WIDTH
	}

	fn color(&self) -> Option<Color> {
		Some(White)
	}

	fn is_bold(&self) -> bool {
		false
	}
}

/// A consistent interface for printing things.
///
/// This prints all non-msg things in Hipcheck's output, ensuring
/// they are given the proper alignment, spacing, color, and bolding.
trait Print {
	/// What text to print.
	fn text(&self) -> String;

	/// What the width of that text should be.
	fn width(&self) -> usize;

	/// What the color should be, if any.
	fn color(&self) -> Option<Color>;

	/// Whether the text should be bold.
	fn is_bold(&self) -> bool;

	/// Print the text out with all the configuration.
	fn print(&self, stream: &mut dyn WriteColor) -> Result<()> {
		stream.set_color(
			ColorSpec::new()
				.set_bold(self.is_bold())
				.set_fg(self.color()),
		)?;

		let to_write = format!("{:>width$}", self.text(), width = self.width());
		log::trace!("writing message part [part='{:?}']", to_write);
		write!(stream, "{}", to_write)?;
		// No need to flush here, as the string at the end of each line
		// will be accompanied by a `flush` call.

		Ok(())
	}
}

/// Maximum width of a title
///
/// Length of " Affiliation Pass" (with space)
const MAX_TITLE_WIDTH: usize = 17;

/// Maximum width of an outcome
///
/// Length of " Pass" / " Fail" / " Skip" (with space)
const OUTCOME_WIDTH: usize = 5;

/// Length of " Done" (with space)
const DONE_WIDTH: usize = 5;

// The following code to handle window sizing and line clearing in a cross-platform manner
// is adapted from the Cargo project (MIT license). https://github.com/rust-lang/cargo

/// Represents the width of the TTY output.
#[derive(Debug)]
#[allow(dead_code)]
enum TtyWidth {
	/// stdout is not a TTY.
	NoTty,
	/// We know the size of the TTY window exactly.
	Known(usize),
	/// We have a guess at the size of the TTY window.
	Guess(usize),
}

#[cfg(unix)]
mod imp {
	use super::TtyWidth;
	use hc_common::{error::Result, log};
	use libc::{ioctl, winsize, STDOUT_FILENO, TIOCGWINSZ};
	use std::mem::zeroed;
	use termcolor::WriteColor;

	/// Determine the width of the TTY.
	#[allow(dead_code)]
	#[allow(clippy::useless_conversion)]
	#[allow(clippy::absurd_extreme_comparisons)]
	pub(super) fn width() -> TtyWidth {
		let mut window_size: winsize = unsafe { zeroed() };

		if unsafe { ioctl(STDOUT_FILENO, TIOCGWINSZ.into(), &mut window_size) } < 0 {
			return TtyWidth::NoTty;
		}

		if window_size.ws_col <= 0 {
			return TtyWidth::NoTty;
		}

		TtyWidth::Known(window_size.ws_col as usize)
	}

	/// Erase the current line in the TTY.
	pub(super) fn erase_line(out: &mut dyn WriteColor) -> Result<()> {
		// This is the ANSI escape code CSI sequence "EL - Erase in Line".
		let to_write = b"\x1B[K";
		log::trace!("writing message part [part='{:?}']", to_write);
		out.write_all(to_write)?;
		out.flush()?;
		Ok(())
	}
}

/// Implementation of TTY width-checking.
#[cfg(windows)]
mod imp {
	use super::TtyWidth;
	use hc_common::{error::Result, log};
	use std::mem::zeroed;
	use std::{cmp, ptr};
	use termcolor::WriteColor;
	use winapi::um::fileapi::*;
	use winapi::um::handleapi::*;
	use winapi::um::processenv::*;
	use winapi::um::winbase::*;
	use winapi::um::wincon::*;
	use winapi::um::winnt::*;

	/// Determine the width of the TTY.
	pub(super) fn width() -> TtyWidth {
		let stdout = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };

		let mut csbi: CONSOLE_SCREEN_BUFFER_INFO = unsafe { zeroed() };

		if unsafe { GetConsoleScreenBufferInfo(stdout, &mut csbi) } != 0 {
			let width = (csbi.srWindow.Right - csbi.srWindow.Left) as usize;
			return TtyWidth::Known(width);
		}

		// On mintty/msys/cygwin based terminals, the above fails with
		// INVALID_HANDLE_VALUE. Use an alternate method which works
		// in that case as well.
		let h = {
			let name = "CONOUT$\0".as_ptr() as *const CHAR;
			let access = GENERIC_READ | GENERIC_WRITE;
			let mode = FILE_SHARE_READ | FILE_SHARE_WRITE;
			let security = ptr::null_mut();
			let disposition = OPEN_EXISTING;
			let flags = 0;
			let template = ptr::null_mut();

			unsafe { CreateFileA(name, access, mode, security, disposition, flags, template) }
		};

		if h == INVALID_HANDLE_VALUE {
			return TtyWidth::NoTty;
		}

		let mut csbi: CONSOLE_SCREEN_BUFFER_INFO = unsafe { zeroed() };
		let rc = unsafe { GetConsoleScreenBufferInfo(h, &mut csbi) };
		unsafe { CloseHandle(h) };

		if rc != 0 {
			let width = (csbi.srWindow.Right - csbi.srWindow.Left) as usize;
			// Unfortunately cygwin/mintty does not set the size of the
			// backing console to match the actual window size. This
			// always reports a size of 80 or 120 (not sure what
			// determines that). Use a conservative max of 60 which should
			// work in most circumstances. ConEmu does some magic to
			// resize the console correctly, but there's no reasonable way
			// to detect which kind of terminal we are running in, or if
			// GetConsoleScreenBufferInfo returns accurate information.

			// Before faulting to at most 60 as the width, let's try one
			// more thing: running `tput cols` and seeing if that returns
			// something usable. If it doesn't, we'll ignore it.
			if let Ok(Ok(width)) = duct::cmd!("tput", "cols")
				.read()
				.map(|v| v.parse::<usize>())
			{
				return TtyWidth::Known(width);
			}

			return TtyWidth::Guess(cmp::min(60, width));
		}

		TtyWidth::NoTty
	}

	/// Erase the current line in the TTY.
	pub(super) fn erase_line(out: &mut dyn WriteColor) -> Result<()> {
		match width() {
			// If we can figure out the width, print a bunch of blanks.
			TtyWidth::Known(max_width) | TtyWidth::Guess(max_width) => {
				let blank = " ".repeat(max_width);
				let to_write = format!("{}\r", blank);

				log::trace!("writing message part [part='{:?}']", to_write);
				write!(out, "{}", to_write)?;
				out.flush()?;
				Ok(())
			}
			// Otherwise, do nothing.
			_ => Ok(()),
		}
	}
}
