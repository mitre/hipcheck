// SPDX-License-Identifier: Apache-2.0
use std::io::Seek;
use std::path::Path;

use anyhow::anyhow;
use jiff::{Unit, Zoned};
use serde::Serialize;

use super::config::{BenchmarkTarget, BenchmarkTargetType};

pub trait FromBenchmarkResult {
	const OUTPUT_FILE: &'static str;
	fn from_benchmark_result(result: &BenchmarkResult) -> Self;
	fn write_to_file(&self, output_dir: &Path) -> anyhow::Result<()>;
}

macro_rules! SingleBenchmarkResult {
	($struct_name:ident, $benchmark_result_field_name: ident, $result_type:ty, $output_file: expr) => {
		#[derive(serde::Serialize, Debug)]
		pub struct $struct_name {
			/// repo type being analyzed, passed as `-t` to `hc check`
			target_type: BenchmarkTargetType,
			/// target passed to `hc check`
			target: String,
			/// reference identifier for the target, passed as `--ref` to `hc check`
			reference: String,
			/// time of the benchmark was created
			record_time: Zoned,
			/// metric being recorded in this struct
			$benchmark_result_field_name: $result_type,
		}

		impl FromBenchmarkResult for $struct_name {
			const OUTPUT_FILE: &'static str = $output_file;

			fn from_benchmark_result(result: &BenchmarkResult) -> Self {
				Self {
					target_type: result.target_type,
					target: result.target.clone(),
					reference: result.reference.clone(),
					record_time: result.record_time.clone(),
					$benchmark_result_field_name: result.$benchmark_result_field_name,
				}
			}

			fn write_to_file(&self, output_dir: &Path) -> anyhow::Result<()> {
				let mut file = std::fs::OpenOptions::new()
					.read(true)
					.append(true)
					.create(true)
					.open(output_dir.join(Self::OUTPUT_FILE))?;

				let mut reader = csv::ReaderBuilder::new().from_reader(&file);
				if reader.records().count() == 0 {
					file.seek(std::io::SeekFrom::Start(0))?;
					let mut header_writer = csv::WriterBuilder::new().from_writer(&file);
					let headers = csv::StringRecord::from(vec![
						"target_type",
						"target",
						"reference",
						"record_time",
						stringify!($benchmark_result_field_name),
					]);
					header_writer.write_record(&headers)?;
					header_writer.flush()?;
				}
				let mut writer = csv::WriterBuilder::new()
					.has_headers(false)
					.from_writer(file);
				writer.serialize(&self)?;
				writer.flush()?;
				Ok(())
			}
		}
	};
}

// create wrappers for each of the metrics being tracked to make it easier to write
// each metric to its own file
SingleBenchmarkResult!(
	AvgWallTimeResult,
	avg_wall_time,
	f32,
	"average_wall_time.csv"
);
SingleBenchmarkResult!(
	AvgMaxRssKb,
	avg_max_rss_in_kb,
	u64,
	"average_max_rss_in_kb.csv"
);
SingleBenchmarkResult!(
	AvgTotalInstructions,
	avg_total_instructions,
	u64,
	"average_total_instructions.csv"
);
SingleBenchmarkResult!(AvgCpuCycles, avg_cpu_cycles, u64, "average_cpu_cycles.csv");

#[derive(Debug)]
pub struct BenchmarkResult {
	/// repo type being analyzed, passed as `-t` to `hc check`
	target_type: BenchmarkTargetType,
	/// target passed to `hc check`
	target: String,
	/// reference identifier for the target, passed as `--ref` to `hc check`
	reference: String,
	/// time of the benchmark was created
	record_time: Zoned,
	/// average wall time across all runs
	avg_wall_time: f32,
	/// average max RSS across all runs
	avg_max_rss_in_kb: u64,
	/// average total instructions across all runs
	avg_total_instructions: u64,
	/// average total number of cpu cycles across all runs
	avg_cpu_cycles: u64,
}

impl BenchmarkResult {
	pub fn new(target: BenchmarkTarget, stats: BenchmarkStats) -> Self {
		Self {
			target_type: target.target_type(),
			target: target.url().to_string(),
			reference: target.reference().to_string(),
			record_time: Zoned::now().round(Unit::Second).unwrap(),
			avg_wall_time: stats.wall_time,
			avg_total_instructions: stats.instructions,
			avg_cpu_cycles: stats.cpu_cycles,
			avg_max_rss_in_kb: stats.max_rss,
		}
	}
}

#[derive(Debug, Default, Serialize)]
pub struct BenchmarkStats {
	/// how long the test took
	pub wall_time: f32,
	/// number of instructions executed during benchmark run
	pub instructions: u64,
	/// number of CPU cycles used during benchmark run
	pub cpu_cycles: u64,
	/// peak memory usage during benchmark run (in KB)
	pub max_rss: u64,
}

impl BenchmarkStats {
	pub fn parse_benchmark_result_stderr(stderr: &str) -> anyhow::Result<Self> {
		let result = BenchmarkStatsParser::new()
			.parse_wall_time(stderr)
			.ok_or(anyhow!("Unable to parse wall time"))?
			.parse_instructions(stderr)
			.ok_or(anyhow!("Unable to parse number of instructions"))?
			.parse_cpu_cycles(stderr)
			.ok_or(anyhow!("Unable to parse CPU cycles"))?
			.parse_max_rss(stderr)
			.ok_or(anyhow!("Unable to parse max RSS"))?
			.build()
			.ok_or(anyhow!("Unable to determine overall result"))?;
		Ok(result)
	}
}

#[derive(Clone, Copy, Debug, Default)]
/// structure for parsing stderr from benchmark results and capturing statistics
pub struct BenchmarkStatsParser {
	wall_time: Option<f32>,
	instructions: Option<u64>,
	cpu_cycles: Option<u64>,
	max_rss: Option<u64>,
}

impl BenchmarkStatsParser {
	fn new() -> Self {
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

	fn parse_max_rss(mut self, stderr: &str) -> Option<Self> {
		let max_rss = stderr
			.lines()
			.find(|line| line.contains("Maximum resident set size (kbytes)"))?
			.split_whitespace()
			.last()?;
		self.max_rss = Some(max_rss.parse().ok()?);
		Some(self)
	}

	fn build(self) -> Option<BenchmarkStats> {
		Some(BenchmarkStats {
			wall_time: self.wall_time?,
			instructions: self.instructions?,
			cpu_cycles: self.cpu_cycles?,
			max_rss: self.max_rss?,
		})
	}
}

#[cfg(test)]
mod test {
	use super::*;

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
		let builder = BenchmarkStatsParser::new()
			.parse_wall_time(TEST_OUTPUT)
			.unwrap();
		assert_eq!(builder.wall_time, Some(11.339_845));
	}

	#[test]
	fn test_parse_instructions() {
		let builder = BenchmarkStatsParser::new()
			.parse_instructions(TEST_OUTPUT)
			.unwrap();
		assert_eq!(builder.instructions, Some(37840778595));
	}

	#[test]
	fn test_parse_cpu_cycles() {
		let builder = BenchmarkStatsParser::new()
			.parse_cpu_cycles(TEST_OUTPUT)
			.unwrap();
		assert_eq!(builder.cpu_cycles, Some(27295917202));
	}

	#[test]
	fn test_parse_max_rss() {
		let builder = BenchmarkStatsParser::new()
			.parse_max_rss(TEST_OUTPUT)
			.unwrap();
		assert_eq!(builder.max_rss, Some(246904));
	}
}
