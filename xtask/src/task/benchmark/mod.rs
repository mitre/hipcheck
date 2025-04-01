// SPDX-License-Identifier: Apache-2.0

use std::{num::NonZeroUsize, path::PathBuf};

#[cfg(not(target_os = "linux"))]
mod non_linux {
	use super::BenchmarkArgs;
	use anyhow::Result;

	pub fn run(_args: BenchmarkArgs) -> Result<()> {
		panic!("benchmark is currently only available on Linux")
	}
}

#[cfg(not(target_os = "linux"))]
pub use non_linux::run;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::run;

#[derive(Debug, clap::Args)]
pub struct BenchmarkArgs {
	/// number of runs to perform for each benchmark target
	#[arg(long = "runs", short = 'r', default_value_t = NonZeroUsize::new(3).unwrap())]
	runs: NonZeroUsize,
	/// path to file contain configuration for running benchmarks
	#[arg(long = "config", short = 'c')]
	config: PathBuf,
	/// path to directory to write results should be written, results will be appended to a CSV
	/// file per metric
	#[arg(long = "output", short = 'o')]
	output_dir: PathBuf,
}
