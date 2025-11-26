// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use clap::Parser;
use hipcheck_sdk::LogLevel;

#[derive(Parser, Debug)]
pub struct Cli {
	#[arg(long)]
	pub port: u16,

	#[arg(long, default_value_t=LogLevel::Error)]
	pub log_level: LogLevel,

	#[arg(trailing_var_arg(true), allow_hyphen_values(true), hide = true)]
	pub unknown_args: Vec<String>,
}

impl Cli {
	pub fn parse_args() -> Result<Self> {
		Ok(Self::try_parse()?)
	}
}
