// SPDX-License-Identifier: Apache-2.0

#![deny(missing_docs)]

use crate::{
	cli::Format,
	error::{Error, Result},
	report::{RecommendationKind, Report},
};
use console::{Emoji, Style, Term};
use indicatif::{MultiProgress, ProgressDrawTarget};
use minijinja::Environment;
use std::{
	fmt::{self, Alignment, Debug, Display, Formatter},
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

/// MiniJinja template for human-readable output
const TEMPLATE: &str = r#"
{{ title("Analyzed") }} '{{ repo_name }}' ({{ repo_head }})
{{ indent() }} using Hipcheck {{ hipcheck_version }}
{{ indent() }} at {{ analyzed_at|datetimeformat(format="[weekday repr:short] [month repr:long] [day padding:none], [year] at [hour padding:none repr:12]:[minute][period case:lower]") }}

{% if passing -%}
{{ title("PassingSection") }}
{% for passed in passing  -%}
{{ title("Passed") }} {{ passed.name }} passed, {{ passed.policy_expr }}
{{ indent() }} {{ passed.message  }}
{% for concern in passed.concerns -%}
{{ indent() }} {{ concern  }}
{% endfor %}
{% endfor %}
{%- endif -%}
{% if failing -%}
{{ title("FailingSection") }}
{% for failed in failing  -%}
{{ title("Failed") }} {{ failed.name }} failed, {{ failed.policy_expr }}
{{ indent() }} {{ failed.message  }}
{% for concern in failed.concerns -%}
{{ indent() }} {{ concern  }}
{% endfor %}
{% endfor %}
{%- endif -%}
{% if errored -%}
{{ title("ErroredSection") }}
{% for error in errored  -%}
{{ title("Errored") }} {{ error.analysis }} error: {{ error.error.msg }}
{% endfor %}
{% endif -%}
{{ title("RecommendationSection") }}
{% if recommendation.kind == "Pass" -%}
{{ title("Pass") }} risk rated as {{ recommendation.risk_score }}, policy was {{ recommendation.risk_policy }}
{% elif recommendation.kind == "Investigate" -%}
{{ title("Investigate") }} risk rated as {{ recommendation.risk_score }}, policy was {{ recommendation.risk_policy }}
{%- endif %}"#;

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
	#[allow(unused)]
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

	/// Print "Config {msg}" with the proper color/styling.
	pub fn print_config(source: impl AsRef<str>) {
		match Shell::get_verbosity() {
			Verbosity::Normal => {
				macros::println!("{:>LEFT_COL_WIDTH$} {}", Title::Config, source.as_ref());
			}
			Verbosity::Quiet | Verbosity::Silent => {}
		}
	}

	/// Print a hipcheck [Error]. Human readable errors will go to the standard error, JSON (regular or full) will go to the standard output.
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

			_ => {
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

				tracing::trace!("writing message part [part='{:?}']", error_json);

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
			Format::Debug => print_json(report),
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
	// Turn the JSON serializable report into a human readable terminal output

	//      Analyzed '<repo_name>' (<repo_head>)
	//               using Hipcheck <hipcheck_version>
	//               at <analyzed_at:pretty_print>
	//
	//       Passing
	//            + mitre/affiliation passed, (lte (count (filter (eq #t) $)) 0)
	//              0 was the number of the repository's contributors flagged as affiliated, which was required to be less than or equal to 0
	//            + mitre/churn passed, (lte (divz (count (filter (gt 3) $)) (count $)) 0.02)
	//              0 was the percent of churn frequencies of each commit in the repository greater than 3, which was required to be less than or equal to 0.02
	//            + mitre/identity passed, (lte (divz (count (filter (eq #t) $)) (count $)) 0.02)
	//              0 was the percent of commits merged by their own author, which was required to be less than or equal to 0.02
	//
	//       Failing
	//            - mitre/entropy failed, (eq 0 (count (filter (gt 8) $)))
	//              Expected the number of entropy calculations of each commit in the repository greater than 8 to be equal to 0, but it was 2
	//
	//            - mitre/activity failed, (lte $ P364D)
	//              Expected span of time that has elapsed since last activity in repo to be less than or equal to 364 days but it was 952 days
	//
	//        Errored
	//            ? mitre/review error: missing GitHub token with permissions for accessing public repository data in config
	//
	//            ? mitre/typo error: can't identify a known language in the repository
	//
	// Recommendation
	//           PASS risk rated as 0.4, policy was (gt 0.5 $)

	// Create the MiniJinja environment
	let mut env = Environment::new();
	// Add additional filters and functions
	minijinja_contrib::add_to_environment(&mut env);
	env.add_function("title", print_title);
	env.add_function("indent", indent);
	// Add template
	env.add_template("human", TEMPLATE)?;
	// Print the report as formatted in the template
	let template = env.get_template("human")?;
	macros::println!("{}", template.render(report)?);

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
	/// "Config"
	Config,
	/// "Passing"
	PassingSection,
	/// "Failing"
	FailingSection,
	/// "Errored"
	ErroredSection,
	/// "Recommendation"
	RecommendationSection,
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
			Config => "Config",
			PassingSection => "Passing",
			FailingSection => "Failing",
			ErroredSection => "Errored",
			RecommendationSection => "Recommendation",
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
			Analyzed | PassingSection | FailingSection | ErroredSection | RecommendationSection => {
				Some(Blue)
			}
			Analyzing | Done | Config => Some(Cyan),
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

fn print_title(title: &str) -> String {
	match title {
		"Analyzed" => format!("{:>LEFT_COL_WIDTH$}", Title::Analyzed),
		"PassingSection" => format!("{:>LEFT_COL_WIDTH$}", Title::PassingSection),
		"FailingSection" => format!("{:>LEFT_COL_WIDTH$}", Title::FailingSection),
		"ErroredSection" => format!("{:>LEFT_COL_WIDTH$}", Title::ErroredSection),
		"RecommendationSection" => format!("{:>LEFT_COL_WIDTH$}", Title::RecommendationSection),
		"Passed" => format!("{:>LEFT_COL_WIDTH$}", Title::Passed),
		"Failed" => format!("{:>LEFT_COL_WIDTH$}", Title::Failed),
		"Errored" => format!("{:>LEFT_COL_WIDTH$}", Title::Errored),
		"Pass" => format!("{:>LEFT_COL_WIDTH$}", Title::Pass),
		"Investigate" => format!("{:>LEFT_COL_WIDTH$}", Title::Investigate),
		// Returns an empty String on a failed match because a minijinja function cannot return a Result or Option
		_ => "".to_string(),
	}
}

fn indent() -> String {
	format!("{:>LEFT_COL_WIDTH$}", "")
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
