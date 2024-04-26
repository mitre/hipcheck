// SPDX-License-Identifier: Apache-2.0

pub use hc_report_builder::{AnyReport, Format, PrReport, Report};
pub use hc_session::{resolve_config, resolve_data, resolve_home, Check, CheckType};
pub use hc_shell::{ColorChoice, Output, Shell, Verbosity};
pub use hc_version as version;

use hc_common::{context::Context as _, error::{Error, Result}};
use hc_report_builder::{build_pr_report, build_report};
use hc_score::{score_pr_results, score_results};
use hc_session::Session;
use std::path::PathBuf;

/// Run Hipcheck.
///
/// Parses arguments, sets up shell output, and then runs the main logic.
#[allow(clippy::too_many_arguments)]
pub fn run(
	output: Output,
	error_output: Output,
	verbosity: Verbosity,
	check: Check,
	config_path: Option<PathBuf>,
	data_path: Option<PathBuf>,
	home_dir: Option<PathBuf>,
	format: Format,
	raw_version: &str,
) -> (Shell, Result<AnyReport>) {
	// Setup wrapper for shell output.
	let shell = Shell::new(output, error_output, verbosity);

	// Run and print / report errors.
	run_with_shell(
		shell,
		check,
		config_path,
		data_path,
		home_dir,
		format,
		raw_version,
	)
}

// This is pub for testing purposes.
/// Now that we're fully-initialized, run Hipcheck's analyses.
#[allow(clippy::too_many_arguments)]
#[doc(hidden)]
pub fn run_with_shell(
	shell: Shell,
	check: Check,
	config_path: Option<PathBuf>,
	data_path: Option<PathBuf>,
	home_dir: Option<PathBuf>,
	format: Format,
	raw_version: &str,
) -> (Shell, Result<AnyReport>) {
	// Initialize the session.
	let session = match Session::new(
		shell,
		&check,
		&check.check_value,
		config_path,
		data_path,
		home_dir,
		format,
		raw_version,
	) {
		Ok(session) => session,
		Err((shell, err)) => return (shell, Err(err)),
	};

	match check.check_type {
		CheckType::RepoSource | CheckType::SpdxDocument => {
			// Run analyses against a repo and score the results (score calls analyses that call metrics).
			let mut phase = match session.shell.phase("analyzing and scoring results") {
				Ok(phase) => phase,
				Err(err) => return (session.end(), Err(err)),
			};

			let scoring = match score_results(&mut phase, &session) {
				Ok(scoring) => scoring,
				_ => {
					return (
						session.end(),
						Err(Error::msg("Trouble scoring and analyzing results")),
					)
				}
			};

			match phase.finish() {
				Ok(()) => {}
				Err(err) => return (session.end(), Err(err)),
			};

			// Build the final report.
			let report =
				match build_report(&session, &scoring).context("failed to build final report") {
					Ok(report) => report,
					Err(err) => return (session.end(), Err(err)),
				};

			(session.end(), Ok(AnyReport::Report(report)))
		}
		CheckType::PackageVersion => {
			// Run analyses against a repo and score the results (score calls analyses that call metrics).
			let mut phase = match session.shell.phase("analyzing and scoring results") {
				Ok(phase) => phase,
				Err(err) => return (session.end(), Err(err)),
			};

			let scoring = match score_results(&mut phase, &session) {
				Ok(scoring) => scoring,
				_ => {
					return (
						session.end(),
						Err(Error::msg("Trouble scoring and analyzing results")),
					)
				}
			};

			match phase.finish() {
				Ok(()) => {}
				Err(err) => return (session.end(), Err(err)),
			};

			// Build the final report.
			let report =
				match build_report(&session, &scoring).context("failed to build final report") {
					Ok(report) => report,
					Err(err) => return (session.end(), Err(err)),
				};

			(session.end(), Ok(AnyReport::Report(report)))
		}
		CheckType::PrUri => {
			// Run analyses against a pull request and score the results (score calls analyses that call metrics).
			let mut phase = match session.shell.phase("scoring and analyzing results") {
				Ok(phase) => phase,
				Err(err) => return (session.end(), Err(err)),
			};

			let score = match score_pr_results(&mut phase, &session) {
				Ok(score) => score,
				_ => {
					return (
						session.end(),
						Err(Error::msg("Trouble scoring and analyzing results")),
					)
				}
			};

			match phase.finish() {
				Ok(()) => {}
				Err(err) => return (session.end(), Err(err)),
			};

			// Build the final report.
			let pr_report =
				match build_pr_report(&session, &score).context("failed to build final report") {
					Ok(pr_report) => pr_report,
					Err(err) => return (session.end(), Err(err)),
				};

			(session.end(), Ok(AnyReport::PrReport(pr_report)))
		}
		_ => (
			session.end(),
			Err(Error::msg(
				"Hipcheck attempted to analyze an unsupported type",
			)),
		),
	}
}

/// Print errors which occur before the `Shell` type can be setup.
pub fn print_error(err: &Error) {
	let mut chain = err.chain();

	// PANIC: First error is guaranteed to be present.
	eprintln!("error: {}", chain.next().unwrap());

	for err in chain {
		eprintln!("       {}", err);
	}
}

pub enum Outcome {
	Ok,
	Err,
}

impl Outcome {
	pub fn exit_code(&self) -> i32 {
		match self {
			Outcome::Ok => 0,
			Outcome::Err => 1,
		}
	}
}
