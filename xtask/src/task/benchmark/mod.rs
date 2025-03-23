// SPDX-License-Identifier: Apache-2.0

use std::{
	process::{Child, Command, Stdio},
	str::FromStr,
	time::Duration,
};

use anyhow::{anyhow, Context, Result};
use config::BenchmarkTargetType;
use which::which;
use xshell::Shell;

use crate::workspace::root;

mod config;

/// binaries required to run the benchmarks
///
/// - `perf` is used to record cpu statistics (cycles, instructions, wall time)
/// - `/usr/bin/time` is used to record max RAM usage via maximum RSS
const REQUIRED_BINARIES: [&str; 2] = ["perf", "/usr/bin/time"];

/// Run the `benchmark` command
pub fn run() -> Result<()> {
	let sh = Shell::new().context("could not init shell")?;
	run_benchmark_suite(&sh)
}

/// Use existing `xshell::Shell` to run `buf lint`
#[cfg(target_os = "linux")]
pub fn run_benchmark_suite(sh: &Shell) -> Result<()> {
	for binary in REQUIRED_BINARIES {
		which(binary).context(format!("could not find '{}'", binary))?;
	}

	// TODO: build everything
	benchmark_single_target(
		"https://github.com/mitre/hipcheck",
		BenchmarkTargetType::Repo,
		"hipcheck-v3.11.0",
	)?;

	Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn run_benchmark_suite(sh: &Shell) -> Result<()> {
	panic!("Runtime benchmarking is only supported on Linux")
}

#[derive(Clone, Copy, Debug)]
pub struct BenchmarkStats {
	cpu_cycles: Option<u64>,
	instructions: u64,
	wall_time: Duration,
}

// TODO: mandate use of a tag/hash
pub fn benchmark_single_target(
	url: &str,
	target_type: BenchmarkTargetType,
	refspec: &str,
) -> Result<BenchmarkResult> {
	// TODO: remove unwrap (check /proc/sys/kernel/perf_event_paranoid)

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
		refspec,
		url,
	];
	let process = Command::new("/usr/bin/time")
		.current_dir(root()?)
		.args(args)
		// the output needed for parsing benchmark results is output to stderr
		.stdout(Stdio::null())
		.stderr(Stdio::piped())
		.spawn()?;

	// TODO: increase process priority

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

	let result =
		BenchmarkResult::parse_benchmark_result_stderr(&String::from_utf8_lossy(&output.stderr))
			.ok_or_else(|| anyhow!("Could not parse stderr"))?;
	eprintln!("Result: {:?}", result);
	Ok(result)
}

#[derive(Debug)]
struct BenchmarkResult {
	wall_time: f32,
	instructions: u64,
	cpu_cycles: u64,
	max_rss: u64,
}

impl BenchmarkResult {
	fn parse_benchmark_result_stderr(stderr: &str) -> Option<Self> {
		BenchmarkResultBuilder::new()
			.parse_wall_time(stderr)?
			.parse_instructions(stderr)?
			.parse_cpu_cycles(stderr)?
			.parse_max_rss(stderr)?
			.build()
	}
}

#[derive(Clone, Copy, Debug, Default)]
struct BenchmarkResultBuilder {
	wall_time: Option<f32>,
	instructions: Option<u64>,
	cpu_cycles: Option<u64>,
	max_rss: Option<u64>,
}

impl BenchmarkResultBuilder {
	pub fn new() -> BenchmarkResultBuilder {
		Self::default()
	}

	fn parse_wall_time(mut self, stderr: &str) -> Option<Self> {
		let amount_of_time = stderr
			.lines()
			.find(|line| line.contains("seconds time elapsed"))?
			.split_whitespace()
			.next()?;
		self.wall_time = Some(amount_of_time.parse().ok()?);
		Some(self)
	}

	fn parse_instructions(mut self, stderr: &str) -> Option<Self> {
		let instructions = stderr
			.lines()
			.find(|line| line.contains("instructions") && line.contains("insn per cycle"))?
			.split_whitespace()
			.next()?;
		self.instructions = Some(instructions.parse().ok()?);
		Some(self)
	}

	fn parse_cpu_cycles(mut self, stderr: &str) -> Option<Self> {
		let cycles = stderr
			.lines()
			.find(|line| line.contains("cycles") && line.contains("GHz"))?
			.split_whitespace()
			.next()?;
		self.cpu_cycles = Some(cycles.parse().ok()?);
		Some(self)
	}

	pub fn parse_max_rss(mut self, stderr: &str) -> Option<Self> {
		let max_rss = stderr
			.lines()
			.find(|line| line.contains("Maximum resident set size (kbytes)"))?
			.split_whitespace()
			.last()?;
		self.max_rss = Some(max_rss.parse().ok()?);
		Some(self)
	}

	pub fn build(self) -> Option<BenchmarkResult> {
		Some(BenchmarkResult {
			wall_time: self.wall_time?,
			instructions: self.instructions?,
			cpu_cycles: self.cpu_cycles?,
			max_rss: self.max_rss?,
		})
	}
}

#[cfg(test)]
mod test {
	use crate::task::benchmark::BenchmarkResultBuilder;

	const TEST_OUTPUT: &str = r#"Performance counter stats for './target/release/hc check --policy ./config/local.release.Hipcheck.kdl https://github.com/mitre/hipcheck':

           6919.11 msec task-clock                       #    0.586 CPUs utilized
             36030      context-switches                 #    5.207 K/sec
               338      cpu-migrations                   #   48.850 /sec
            323800      page-faults                      #   46.798 K/sec
       27295917202      cycles                           #    3.945 GHz
       37840778595      instructions                     #    1.39  insn per cycle
        7383582407      branches                         #    1.067 G/sec
         258357252      branch-misses                    #    3.50% of all branches

      11.339844280 seconds time elapsed

       0.985113000 seconds user
       0.406738000 seconds sys


        Command being timed: "perf stat --summary ./target/release/hc check --policy ./config/local.release.Hipcheck.kdl https://github.com/mitre/hipcheck"
        User time (seconds): 0.98
        System time (seconds): 0.41
        Percent of CPU this job got: 11%
        Elapsed (wall clock) time (h:mm:ss or m:ss): 0:11.82
        Average shared text size (kbytes): 0
        Average unshared data size (kbytes): 0
        Average stack size (kbytes): 0
        Average total size (kbytes): 0
        Maximum resident set size (kbytes): 246904
        Average resident set size (kbytes): 0
        Major (requiring I/O) page faults: 0
        Minor (reclaiming a frame) page faults: 134964
        Voluntary context switches: 13760
        Involuntary context switches: 18
        Swaps: 0
        File system inputs: 656
        File system outputs: 220192
        Socket messages sent: 0
        Socket messages received: 0
        Signals delivered: 0
        Page size (bytes): 4096
        Exit status: 0"#;

	#[test]
	fn test_parse_wall_time() {
		let builder = BenchmarkResultBuilder::new()
			.parse_wall_time(TEST_OUTPUT)
			.unwrap();
		assert_eq!(builder.wall_time, Some(11.339844280));
	}

	#[test]
	fn test_parse_instructions() {
		let builder = BenchmarkResultBuilder::new()
			.parse_instructions(TEST_OUTPUT)
			.unwrap();
		assert_eq!(builder.instructions, Some(37840778595));
	}

	#[test]
	fn test_parse_cpu_cycles() {
		let builder = BenchmarkResultBuilder::new()
			.parse_cpu_cycles(TEST_OUTPUT)
			.unwrap();
		assert_eq!(builder.cpu_cycles, Some(27295917202));
	}

	#[test]
	fn test_parse_max_rss() {
		let builder = BenchmarkResultBuilder::new()
			.parse_max_rss(TEST_OUTPUT)
			.unwrap();
		assert_eq!(builder.max_rss, Some(246904));
	}
}
