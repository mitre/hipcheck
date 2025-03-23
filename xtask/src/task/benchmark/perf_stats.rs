// SPDX-License-Identifier: Apache-2.0

// This code was heavily inspired by https://github.com/rust-lang/rustc-perf

use std::os::unix::raw::pid_t;

use anyhow::Result;
use perf_event::{events::Hardware, Builder, Counter, Group};

pub fn create_group() -> Result<Group> {
	match Group::new() {
		Ok(group) => Ok(group),
		Err(error) => {
			let path = "/proc/sys/kernel/perf_event_paranoid";
			let level = std::fs::read_to_string(path).unwrap_or_else(|_| "unknown".to_string());
			let level = level.trim();
			Err(anyhow::anyhow!(
				"Cannot create perf_event group ({:?}). Current value of {} is {}.
Try lowering it with `sudo bash -c 'echo -1 > /proc/sys/kernel/perf_event_paranoid'`.",
				error,
				path,
				level
			))
		}
	}
}

pub struct HardwareCounters {
	pub cpu_cycles: Option<Counter>,
	pub total_instructions: Option<Counter>,
}

pub fn prepare_counters(group: &mut Group, target_pid: u32) -> anyhow::Result<HardwareCounters> {
	let mut add_event = |event: Hardware| match Builder::new()
		.observe_pid(i32::try_from(target_pid).unwrap())
		.group(group)
		.kind(event)
		.build()
	{
		Ok(counter) => Some(counter),
		Err(error) => {
			log::warn!(
				"Could not add counter {:?}: {:?}. Maybe the CPU doesn't support it?",
				event,
				error
			);
			None
		}
	};

	let cpu_cycles = add_event(Hardware::CPU_CYCLES);
	let total_instructions = add_event(Hardware::INSTRUCTIONS);

	Ok(HardwareCounters {
		cpu_cycles,
		total_instructions,
	})
}
