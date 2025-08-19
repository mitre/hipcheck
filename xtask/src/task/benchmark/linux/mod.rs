// SPDX-License-Identifier: Apache-2.0
use std::{
	fs::File,
	io::Read,
	num::NonZeroUsize,
	path::Path,
	process::{Command, Stdio},
	str::FromStr,
};

use anyhow::{Context, Result, anyhow};
use config::{BenchmarkTarget, BenchmarkTargets};
use results::{
	AvgCpuCycles, AvgMaxRssKb, AvgTotalInstructions, AvgWallTimeResult, BenchmarkResult,
	BenchmarkStats, FromBenchmarkResult,
};
use which::which;

use crate::workspace::root;
use crate::{BuildPkg, BuildProfile};

use crate::BenchmarkArgs;

mod config;
mod results;

/// binaries required to run the benchmarks
///
/// - `perf` is used to record cpu statistics (cycles, instructions)
/// - `/usr/bin/time` is used to record max RAM usage via maximum RSS and wall time
const REQUIRED_BINARIES: [&str; 2] = ["perf", "/usr/bin/time"];

fn check_preconditions() -> Result<()> {
	for binary in REQUIRED_BINARIES {
		which(binary).context(format!("could not find '{}'", binary))?;
	}
	// verify /proc/sys/kernel/perf_event_paranoid is set to a level that will allow tracing of
	// perf events
	let mut contents = String::new();
	File::open("/proc/sys/kernel/perf_event_paranoid")?.read_to_string(&mut contents)?;
	let perf_event_level = i32::from_str(contents.trim())?;
	if perf_event_level > 2 {
		return Err(anyhow!(
			"/proc/sys/kernel/perf_event_paranoid greater than 2; 'sudo sysctl -w kernel.perf_event_paranoid=2' to fix"
		));
	}
	Ok(())
}

pub fn run(args: BenchmarkArgs) -> Result<()> {
	check_preconditions()?;
	let targets = BenchmarkTargets::from_file(args.config.as_path())?;
	// build release versions of everything
	crate::task::build::run(crate::BuildArgs {
		profile: BuildProfile::Release,
		pkg: vec![BuildPkg::All],
		timings: false,
	})?;
	run_benchmark_suite(targets, args.runs, &args.output_dir)
}

/// Use existing `xshell::Shell` to run `buf lint`
fn run_benchmark_suite(
	targets: BenchmarkTargets,
	runs_per_target: NonZeroUsize,
	output_dir: &Path,
) -> Result<()> {
	let total_benchmarks = targets.0.len();
	let mut results = Vec::with_capacity(targets.0.len());
	for (idx, target) in targets.0.into_iter().enumerate() {
		eprintln!("Starting Benchmark [{} of {}]", idx + 1, total_benchmarks);
		let result = benchmark_target(target, runs_per_target)?;
		eprintln!(
			"Benchmark Result [{} of {}]: {:?}",
			idx + 1,
			total_benchmarks,
			result
		);
		results.push(result);
	}

	std::fs::create_dir_all(output_dir)?;

	// write each of the metrics to a file
	results
		.iter()
		.try_for_each(|res| AvgCpuCycles::from_benchmark_result(res).write_to_file(output_dir))?;
	results.iter().try_for_each(|res| {
		AvgTotalInstructions::from_benchmark_result(res).write_to_file(output_dir)
	})?;
	results.iter().try_for_each(|res| {
		AvgWallTimeResult::from_benchmark_result(res).write_to_file(output_dir)
	})?;
	results
		.iter()
		.try_for_each(|res| AvgMaxRssKb::from_benchmark_result(res).write_to_file(output_dir))?;

	eprintln!("Wrote results to {:?}", output_dir);
	Ok(())
}

// Run `runs` number of benchmarks for a target repo and gather the results
fn benchmark_target(
	target: BenchmarkTarget,
	runs: NonZeroUsize,
) -> anyhow::Result<BenchmarkResult> {
	let mut results = Vec::with_capacity(runs.into());
	for run in 1..(usize::from(runs)) + 1 {
		eprintln!(
			"Beginning run {run} of {runs} for {} reference: {}",
			target.url(),
			target.reference()
		);
		let result = benchmark_single_target(target.clone())?;
		results.push(result);
	}
	let mut stats = results
		.iter()
		.fold(BenchmarkStats::default(), |mut total, indiv| {
			total.cpu_cycles += indiv.cpu_cycles;
			total.instructions += indiv.instructions;
			total.max_rss += indiv.max_rss;
			total.wall_time += indiv.wall_time;
			total
		});
	stats.instructions /= results.len() as u64;
	stats.cpu_cycles /= results.len() as u64;
	stats.max_rss /= results.len() as u64;
	stats.wall_time /= results.len() as f32;
	let result = BenchmarkResult::new(target.clone(), stats);
	Ok(result)
}

// NOTE: this function intentionally takes a mandatory reference to ensure benchmarks are able
// to be compared
fn benchmark_single_target(target: BenchmarkTarget) -> Result<BenchmarkStats> {
	let target_type = target.target_type().to_string();
	let args = vec![
		"-v",
		"perf",
		"stat",
		"--summary",
		"./target/release/hc",
		"check",
		"--policy",
		"./config/local.release.Hipcheck.kdl",
		"--target",
		target_type.as_str(),
		"--ref",
		target.reference(),
		target.url(),
	];
	let process = Command::new("/usr/bin/time")
		.current_dir(root()?)
		.args(args)
		// the output needed for parsing benchmark results is output to stderr
		.stdout(Stdio::null())
		.stderr(Stdio::piped())
		.spawn()?;

	// just capture the result, do nothing with it inside active counters to avoid inclusion in
	// measurment
	let result = process.wait_with_output();

	let output = match result {
		Ok(output) => {
			if !output.status.success() {
				return Err(anyhow!("hc check did not exit successfully"));
			}
			output
		}
		Err(e) => return Err(anyhow!("hc errored out: {e}")),
	};

	BenchmarkStats::parse_benchmark_result_stderr(&String::from_utf8_lossy(&output.stderr))
}
