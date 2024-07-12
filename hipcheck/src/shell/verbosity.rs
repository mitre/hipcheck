//! Utilities for managing and controlling the verbosity of hipcheck.

use super::Shell;

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
		if quiet {
			Verbosity::Quiet
		} else {
			Verbosity::Normal
		}
	}
}

/// A [SilenceGuard] is created by calling [Shell::silence], which returns an opaque
/// value of this type. Once that value is [drop]ped, the global [Shell]'s verbosity is set to
/// whatever it was prior to calling [Shell::silence].
#[derive(Debug)]
pub struct SilenceGuard {
	pub(super) previous_verbosity: Verbosity,
}

impl Drop for SilenceGuard {
	fn drop(&mut self) {
		Shell::set_verbosity(self.previous_verbosity);
	}
}
