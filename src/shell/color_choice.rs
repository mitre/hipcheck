// SPDX-License-Identifier: Apache-2.0

//! Utilities for handling whether or not to use color while printing output.

use crate::error::{Error, Result};
use std::str::FromStr;

/// Selection of whether the CLI output should use color.
#[derive(Debug, Default, Copy, Clone, PartialEq, clap::ValueEnum)]
pub enum ColorChoice {
	/// Always use color output
	Always,
	/// Never use color output
	Never,
	/// Guess whether to use color output
	#[default]
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
