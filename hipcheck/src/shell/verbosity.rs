//! Utilities for managing and controlling the verbosity of hipcheck.

/// How verbose CLI output should be.
#[derive(Debug, Default, Copy, Clone, PartialEq, clap::ValueEnum)]
pub enum Verbosity {
	/// Output results, not progress indicators.
	Quiet,
	/// Output results and progress indicators.
	#[default]
	Normal,
	// This one is only used in testing.
	/// Do not output anything.
	#[value(hide = true)]
	Silent,
}

impl Verbosity {
	/// [Verbosity::Quiet] if `quiet` is true, [Verbosity::Normal] otherwise.
	pub fn use_quiet(quiet: bool) -> Verbosity {
		quiet
			.then_some(Verbosity::Quiet)
			.unwrap_or(Verbosity::Normal)
	}
}
