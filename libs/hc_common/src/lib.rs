// SPDX-License-Identifier: Apache-2.0

//! Provides common access to third-party crates and other
//! functionality used widely throughout Hipcheck.

pub mod analysis;
pub mod command_util;
pub mod config;
pub mod context;
pub mod data;
pub mod error;
pub mod filesystem;
pub mod pathbuf;
pub mod report;
pub mod shell;
pub mod test_util;
#[cfg(test)]
mod tests;
mod try_any;
mod try_filter;
pub mod version;

pub use chrono;
pub use lazy_static;
pub use log;
pub use ordered_float;
pub use salsa;
pub use schemars;
pub use semver;
pub use serde_json;
pub use try_any::TryAny;
pub use try_filter::{FallibleFilter, TryFilter};
pub use url;
pub use which;

use crate::analysis::{
	report_builder::{build_pr_report, build_report},
	score::{score_pr_results, score_results},
	session::Session,
};
pub use crate::analysis::{
	report_builder::{AnyReport, Format, PrReport, Report},
	session::{resolve_config, resolve_data, resolve_home, Check, CheckType},
};
pub use crate::shell::{ColorChoice, Output, Shell, Verbosity};
use crate::{
	context::Context as _,
	error::{Error, Result},
};
use std::path::PathBuf;

/// An `f64` that is never `NaN`.
pub type F64 = ordered_float::NotNan<f64>;

//Global variables for toml files per issue 157 config updates
pub const LANGS_FILE: &str = "Langs.toml";
pub const BINARY_CONFIG_FILE: &str = "Binary.toml";
pub const TYPO_FILE: &str = "Typos.toml";
pub const ORGS_FILE: &str = "Orgs.toml";
pub const HIPCHECK_TOML_FILE: &str = "Hipcheck.toml";

// Constants for exiting with error codes.
/// Indicates the program failed.
pub const EXIT_FAILURE: i32 = 1;

/// Indicates the program succeeded.
pub const EXIT_SUCCESS: i32 = 0;

//used in hc_session::pm and main.rs, global variables for hc check CheckKindHere node-ipc@9.2.1
pub enum CheckKind {
	Repo,
	Request,
	Patch,
	Maven,
	Npm,
	Pypi,
	Spdx,
}

impl CheckKind {
	pub const fn name(&self) -> &'static str {
		match self {
			CheckKind::Repo => "repo",
			CheckKind::Request => "request",
			CheckKind::Patch => "patch",
			CheckKind::Maven => "maven",
			CheckKind::Npm => "npm",
			CheckKind::Pypi => "pypi",
			CheckKind::Spdx => "spdx",
		}
	}
}

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
