// SPDX-License-Identifier: Apache-2.0

use crate::{error::Result, hc_error, plugin::PluginExecutor, util::fs::read_string};
use hipcheck_kdl::kdl::{KdlDocument, KdlNode, KdlValue};
use hipcheck_kdl::{extract_data, ParseKdlNode};
use std::{env, path::Path, str::FromStr};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginBackoffInterval(u64);

impl ParseKdlNode for PluginBackoffInterval {
	fn kdl_key() -> &'static str {
		"backoff-interval"
	}

	fn parse_node(node: &KdlNode) -> Option<Self> {
		if node.name().to_string().as_str() != Self::kdl_key() {
			return None;
		}
		let specified_duration = node.entries().first()?;
		let micros = match specified_duration.value() {
			// Value should be greater than 0
			KdlValue::Integer(micros) => {
				let micros = *micros;
				if micros.is_positive() {
					micros as u64
				} else {
					return None;
				}
			}
			_ => return None,
		};
		Some(PluginBackoffInterval(micros))
	}
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginMaxSpawnAttempts(usize);

impl ParseKdlNode for PluginMaxSpawnAttempts {
	fn kdl_key() -> &'static str {
		"max-spawn-attempts"
	}

	fn parse_node(node: &KdlNode) -> Option<Self> {
		if node.name().to_string().as_str() != Self::kdl_key() {
			return None;
		}
		let specified_attempts = node.entries().first()?;
		let attempts = match specified_attempts.value() {
			// Value should be greater than 0
			KdlValue::Integer(attempts) => {
				let attempts = *attempts;
				if attempts.is_positive() {
					attempts as usize
				} else {
					return None;
				}
			}
			_ => return None,
		};
		Some(PluginMaxSpawnAttempts(attempts))
	}
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginMaxConnectionAttempts(usize);

impl ParseKdlNode for PluginMaxConnectionAttempts {
	fn kdl_key() -> &'static str {
		"max-conn-attempts"
	}

	fn parse_node(node: &KdlNode) -> Option<Self> {
		if node.name().to_string().as_str() != Self::kdl_key() {
			return None;
		}
		let specified_attempts = node.entries().first()?;
		let attempts = match specified_attempts.value() {
			// Value should be greater than 0
			KdlValue::Integer(attempts) => {
				let attempts = *attempts;
				if attempts.is_positive() {
					attempts as usize
				} else {
					return None;
				}
			}
			_ => return None,
		};
		Some(PluginMaxConnectionAttempts(attempts))
	}
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginJitterPercent(u8);

impl ParseKdlNode for PluginJitterPercent {
	fn kdl_key() -> &'static str {
		"jitter-percent"
	}

	fn parse_node(node: &KdlNode) -> Option<Self> {
		if node.name().to_string().as_str() != Self::kdl_key() {
			return None;
		}
		let specified_percentage = node.entries().first()?;
		let percent = match specified_percentage.value() {
			// Value should be greater than 0
			KdlValue::Integer(percent) => {
				let percent = *percent;
				if percent.is_positive() {
					percent as u8
				} else {
					return None;
				}
			}
			_ => return None,
		};
		Some(PluginJitterPercent(percent))
	}
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginMsgBufferSize(usize);

impl ParseKdlNode for PluginMsgBufferSize {
	fn kdl_key() -> &'static str {
		"grpc-msg-buffer-size"
	}

	fn parse_node(node: &KdlNode) -> Option<Self> {
		if node.name().to_string().as_str() != Self::kdl_key() {
			return None;
		}
		let specified_size = node.entries().first()?;
		let size = match specified_size.value() {
			// Value should be greater than 0
			KdlValue::Integer(size) => {
				let size = *size;
				if size.is_positive() {
					size as usize
				} else {
					return None;
				}
			}
			_ => return None,
		};
		Some(PluginMsgBufferSize(size))
	}
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginConfig {
	pub backoff_micros: u64,
	pub max_spawn_attempts: usize,
	pub max_conn_attempts: usize,
	pub jitter_percent: u8,
	pub grpc_buffer_size: usize,
}

impl PluginConfig {
	pub fn new(
		backoff_micros: u64,
		max_spawn_attempts: usize,
		max_conn_attempts: usize,
		jitter_percent: u8,
		grpc_buffer_size: usize,
	) -> Self {
		Self {
			backoff_micros,
			max_spawn_attempts,
			max_conn_attempts,
			jitter_percent,
			grpc_buffer_size,
		}
	}
}

impl Default for PluginConfig {
	fn default() -> Self {
		let backoff = if cfg!(target_os = "macos") {
			1000000
		} else {
			10000
		};
		PluginConfig::new(backoff, 3, 5, 10, 10)
	}
}

impl ParseKdlNode for PluginConfig {
	fn kdl_key() -> &'static str {
		"plugin"
	}

	fn parse_node(node: &KdlNode) -> Option<Self> {
		if node.name().to_string().as_str() != Self::kdl_key() {
			return None;
		}
		let nodes = node.children()?.nodes();

		// extract the configured plugin data from the child
		let backoff: PluginBackoffInterval = extract_data(nodes)?;
		let max_spawn: PluginMaxSpawnAttempts = extract_data(nodes)?;
		let max_conn: PluginMaxConnectionAttempts = extract_data(nodes)?;
		let jitter: PluginJitterPercent = extract_data(nodes)?;
		let grpc_buffer: PluginMsgBufferSize = extract_data(nodes)?;

		Some(Self::new(
			backoff.0,
			max_spawn.0,
			max_conn.0,
			jitter.0,
			grpc_buffer.0,
		))
	}

	// add to_kdl(&self) & to_kdl_formatted_string from plugin manifest
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ExecConfig {
	pub plugin_data: PluginConfig,
	// Any new configurable data forms can be added here
}

impl ExecConfig {
	pub fn from_file<P>(path: P) -> Result<Self>
	where
		P: AsRef<Path>,
	{
		Self::from_str(&read_string(path)?)
	}

	pub fn find_file() -> Result<Self> {
		// Locate file
		let mut exec_file = "Exec.kdl";
		let mut curr_dir = env::current_dir()?;
		let file = curr_dir.join(exec_file);
		let file_path = file.as_path();
		if file_path.exists() {
			// Parse found file
			log::info!("Using Exec Config at {:?}", file_path);
			return Self::from_file(file_path);
		}

		// Walk the directory tree for the file
		exec_file = ".hipcheck/Exec.kdl";
		loop {
			let target_path = curr_dir.join(exec_file);
			let target_ref = target_path.as_path();
			if target_ref.exists() {
				// Parse found file
				log::info!("Using Exec Config at {:?}", target_ref);
				return Self::from_file(target_ref);
			}
			if let Some(parent) = curr_dir.parent() {
				curr_dir = parent.to_path_buf();
			} else {
				// If file not found, use default values
				log::info!("Using a default Exec Config");
				return Ok(Self::default());
			}
		}
	}

	pub fn get_plugin_executor(&self) -> Result<PluginExecutor> {
		let plugin_data = &self.plugin_data;
		PluginExecutor::new(
			/* max_spawn_attempts */ plugin_data.max_spawn_attempts,
			/* max_conn_attempts */ plugin_data.max_conn_attempts,
			/* port_range */ 40000..u16::MAX,
			/* backoff_interval_micros */ plugin_data.backoff_micros,
			/* jitter_percent */ plugin_data.jitter_percent,
			/*grpc_buffer*/ plugin_data.grpc_buffer_size,
		)
	}
}

impl FromStr for ExecConfig {
	type Err = crate::Error;

	fn from_str(s: &str) -> Result<Self> {
		let document = KdlDocument::from_str(s)
			.map_err(|e| hc_error!("Error parsing exec config file: {}", e))?;
		let nodes = document.nodes();
		let plugin_data: PluginConfig = extract_data(nodes).unwrap();
		// Future config nodes will be here
		Ok(Self { plugin_data })
	}
}

#[cfg(test)]
mod test {
	use std::path::PathBuf;

	use super::*;
	use pathbuf::pathbuf;

	#[test]
	fn test_optional_parsing_plugin_buffer_size() {
		let data = "jitter-percent 10";
		let node = KdlNode::from_str(data).unwrap();
		assert_eq!(None, PluginMsgBufferSize::parse_node(&node))
	}

	#[test]
	fn test_parsing_plugin_config() {
		let data = r#"plugin {
            backoff-interval 100000
            max-spawn-attempts 3
            max-conn-attempts 5
            jitter-percent 10
            grpc-msg-buffer-size 10
        }"#;
		let node = KdlNode::from_str(data).unwrap();
		let expected = PluginConfig::new(100000, 3, 5, 10, 10);

		assert_eq!(expected, PluginConfig::parse_node(&node).unwrap())
	}

	#[test]
	fn test_parsing_exec_config_from_str() {
		let data = r#"plugin {
			backoff-interval 100000
			max-spawn-attempts 3
			max-conn-attempts 5
			jitter-percent 10
			grpc-msg-buffer-size 10
		}"#;
		let exec_config = ExecConfig::from_str(data).unwrap();
		assert_eq!(exec_config.plugin_data.backoff_micros, 100000);
		assert_eq!(exec_config.plugin_data.max_spawn_attempts, 3);
		assert_eq!(exec_config.plugin_data.max_conn_attempts, 5);
		assert_eq!(exec_config.plugin_data.jitter_percent, 10);
		assert_eq!(exec_config.plugin_data.grpc_buffer_size, 10);
	}

	#[test]
	fn test_read_exec_config_file() {
		let root = workspace_dir();
		let path = pathbuf![&root, "config", "Exec.kdl"];
		let config = ExecConfig::from_file(path);
		assert!(config.is_ok())
	}

	// Adapted from https://stackoverflow.com/a/74942075
	// Licensed CC BY-SA 4.0
	fn workspace_dir() -> PathBuf {
		let output = std::process::Command::new(env!("CARGO"))
			.arg("locate-project")
			.arg("--workspace")
			.arg("--message-format=plain")
			.output()
			.unwrap()
			.stdout;
		let cargo_path = Path::new(std::str::from_utf8(&output).unwrap().trim());
		cargo_path.parent().unwrap().to_path_buf()
	}
}
