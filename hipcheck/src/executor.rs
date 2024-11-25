// SPDX-License-Identifier: Apache-2.0
// reference hipcheck/src/plugin/plugin_manifest.rs
use crate::{
	error::Error,
	hc_error,
	util::{
		fs::read_string,
		kdl::{extract_data, ParseKdlNode},
	},
};
use kdl::{KdlDocument, KdlNode, KdlValue};
use std::{path::Path, str::FromStr};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginBackoffInterval {
	/// size of the downloaded artifact, in bytes
	pub micros: u64,
}

impl PluginBackoffInterval {
	#[cfg(test)]
	pub fn new(micros: u64) -> Self {
		Self { micros }
	}
}

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
			KdlValue::Base10(micros) => {
				let micros = *micros;
				if micros.is_positive() {
					micros as u64
				} else {
					return None;
				}
			}
			_ => return None,
		};
		Some(PluginBackoffInterval { micros })
	}
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginMaxSpawnAttempts {
	/// the number of spawns to attempt
	pub attempts: usize,
}

impl PluginMaxSpawnAttempts {
	#[cfg(test)]
	pub fn new(attempts: usize) -> Self {
		Self { attempts }
	}
}

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
			KdlValue::Base10(attempts) => {
				let attempts = *attempts;
				if attempts.is_positive() {
					attempts as usize
				} else {
					return None;
				}
			}
			_ => return None,
		};
		Some(PluginMaxSpawnAttempts { attempts })
	}
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginMaxConnectionAttempts {
	/// the number of spawns to attempt
	pub attempts: usize,
}

impl PluginMaxConnectionAttempts {
	#[cfg(test)]
	pub fn new(attempts: usize) -> Self {
		Self { attempts }
	}
}

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
			KdlValue::Base10(attempts) => {
				let attempts = *attempts;
				if attempts.is_positive() {
					attempts as usize
				} else {
					return None;
				}
			}
			_ => return None,
		};
		Some(PluginMaxConnectionAttempts { attempts })
	}
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginJitterPercent {
	/// the number of spawns to attempt
	pub percent: u8,
}

impl PluginJitterPercent {
	#[cfg(test)]
	pub fn new(percent: u8) -> Self {
		Self { percent }
	}
}

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
			KdlValue::Base10(percent) => {
				let percent = *percent;
				if percent.is_positive() {
					percent as u8
				} else {
					return None;
				}
			}
			_ => return None,
		};
		Some(PluginJitterPercent { percent })
	}
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginMsgBufferSize {
	/// size of the buffer for the grpc buffer
	pub size: usize,
}

impl PluginMsgBufferSize {
	#[cfg(test)]
	pub fn new(size: usize) -> Self {
		Self { size }
	}
}

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
			KdlValue::Base10(size) => {
				let size = *size;
				if size.is_positive() {
					size as usize
				} else {
					return None;
				}
			}
			_ => return None,
		};
		Some(PluginMsgBufferSize { size })
	}
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginConfig {
	pub backoff: PluginBackoffInterval,
	pub max_spawn: PluginMaxSpawnAttempts,
	pub max_conn: PluginMaxConnectionAttempts,
	pub jitter: PluginJitterPercent,
	pub grpc_buffer: PluginMsgBufferSize,
}

impl PluginConfig {
	#[cfg(test)]
	pub fn new(
		backoff: PluginBackoffInterval,
		max_spawn: PluginMaxSpawnAttempts,
		max_conn: PluginMaxConnectionAttempts,
		jitter: PluginJitterPercent,
		grpc_buffer: PluginMsgBufferSize,
	) -> Self {
		Self {
			backoff,
			max_spawn,
			max_conn,
			jitter,
			grpc_buffer,
		}
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

		Some(Self {
			backoff,
			max_spawn,
			max_conn,
			jitter,
			grpc_buffer,
		})
	}

	// add to_kdl(&self) & to_kdl_formatted_string from plugin manifest
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecConfig {
	pub plugin_data: PluginConfig,
	// Any new configurable data forms can be added here
}

impl ExecConfig {
	pub fn from_file<P>(path: P) -> Result<Self, Error>
	where
		P: AsRef<Path>,
	{
		Self::from_str(&read_string(path)?)
	}
}

impl FromStr for ExecConfig {
	type Err = crate::Error;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		let document = KdlDocument::from_str(s)
			.map_err(|e| hc_error!("Error parsing exec config file: {}", e))?;
		let nodes = document.nodes();
		let plugin_data: PluginConfig = extract_data(nodes).unwrap();
		// added config groups will here
		Ok(Self { plugin_data })
	}
}

#[cfg(test)]
mod test {
	use std::path::PathBuf;

	use super::*;
	use pathbuf::pathbuf;

	#[test]
	fn test_parsing_plugin_backoff_interval() {
		let data = "backoff-interval 100000";
		let node = KdlNode::from_str(data).unwrap();
		assert_eq!(
			PluginBackoffInterval::new(100000),
			PluginBackoffInterval::parse_node(&node).unwrap()
		)
	}

	#[test]
	fn test_parsing_plugin_max_spawns() {
		let data = "max-spawn-attempts 3";
		let node = KdlNode::from_str(data).unwrap();
		assert_eq!(
			PluginMaxSpawnAttempts::new(3),
			PluginMaxSpawnAttempts::parse_node(&node).unwrap()
		)
	}

	#[test]
	fn test_parsing_plugin_max_connections() {
		let data = "max-conn-attempts 5";
		let node = KdlNode::from_str(data).unwrap();
		assert_eq!(
			PluginMaxConnectionAttempts::new(5),
			PluginMaxConnectionAttempts::parse_node(&node).unwrap()
		)
	}

	#[test]
	fn test_parsing_plugin_jitter_percent() {
		let data = "jitter-percent 10";
		let node = KdlNode::from_str(data).unwrap();
		assert_eq!(
			PluginJitterPercent::new(10),
			PluginJitterPercent::parse_node(&node).unwrap()
		)
	}

	#[test]
	fn test_parsing_plugin_buffer_size() {
		let data = "grpc-msg-buffer-size 10";
		let node = KdlNode::from_str(data).unwrap();
		assert_eq!(
			PluginMsgBufferSize::new(10),
			PluginMsgBufferSize::parse_node(&node).unwrap()
		)
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
		let backoff = PluginBackoffInterval::new(100000);
		let max_spawn = PluginMaxSpawnAttempts::new(3);
		let max_conn = PluginMaxConnectionAttempts::new(5);
		let jitter = PluginJitterPercent::new(10);
		let grpc_buffer = PluginMsgBufferSize::new(10);

		let expected = PluginConfig::new(backoff, max_spawn, max_conn, jitter, grpc_buffer);

		assert_eq!(expected, PluginConfig::parse_node(&node).unwrap())
	}

	#[test]
	fn test_parsing_plugin_config_backoff() {
		let data = r#"plugin {
			backoff-interval 100000
			max-spawn-attempts 3
			max-conn-attempts 5
			jitter-percent 10
			grpc-msg-buffer-size 10
		}"#;
		let node = KdlNode::from_str(data).unwrap();
		let parsed_node = PluginConfig::parse_node(&node).unwrap();

		assert_eq!(parsed_node.backoff.micros, 100000);
	}

	#[test]
	fn test_parsing_plugin_config_max_spawn() {
		let data = r#"plugin {
			backoff-interval 100000
			max-spawn-attempts 3
			max-conn-attempts 5
			jitter-percent 10
			grpc-msg-buffer-size 10
		}"#;
		let node = KdlNode::from_str(data).unwrap();
		let parsed_node = PluginConfig::parse_node(&node).unwrap();

		assert_eq!(parsed_node.max_spawn.attempts, 3);
	}

	#[test]
	fn test_parsing_plugin_config_max_conn() {
		let data = r#"plugin {
			backoff-interval 100000
			max-spawn-attempts 3
			max-conn-attempts 5
			jitter-percent 10
			grpc-msg-buffer-size 10
		}"#;
		let node = KdlNode::from_str(data).unwrap();
		let parsed_node = PluginConfig::parse_node(&node).unwrap();

		assert_ne!(parsed_node.max_conn.attempts, 3);
	}

	#[test]
	fn test_parsing_plugin_config_jitter() {
		let data = r#"plugin {
			backoff-interval 100000
			max-spawn-attempts 3
			max-conn-attempts 5
			jitter-percent 10
			grpc-msg-buffer-size 10
		}"#;
		let node = KdlNode::from_str(data).unwrap();
		let parsed_node = PluginConfig::parse_node(&node).unwrap();

		assert_eq!(parsed_node.jitter.percent, 10);
	}

	#[test]
	fn test_parsing_plugin_config_buffer() {
		let data = r#"plugin {
			backoff-interval 100000
			max-spawn-attempts 3
			max-conn-attempts 5
			jitter-percent 10
			grpc-msg-buffer-size 10
		}"#;
		let node = KdlNode::from_str(data).unwrap();
		let parsed_node = PluginConfig::parse_node(&node).unwrap();

		assert_eq!(parsed_node.grpc_buffer.size, 10);
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
		assert_eq!(exec_config.plugin_data.backoff.micros, 100000);
		assert_eq!(exec_config.plugin_data.max_spawn.attempts, 3);
		assert_eq!(exec_config.plugin_data.max_conn.attempts, 5);
		assert_eq!(exec_config.plugin_data.jitter.percent, 10);
		assert_eq!(exec_config.plugin_data.grpc_buffer.size, 10);
	}

	#[test]
	fn test_read_exec_config_file() {
		let root = workspace_dir();
		let path = pathbuf![&root, "config", "Config.kdl"];
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

	#[test]
	fn test_parsing_exec_config_from_file() {
		let root = workspace_dir();
		let path = pathbuf![&root, "config", "Config.kdl"];
		let config = ExecConfig::from_file(path).unwrap();

		assert_eq!(config.plugin_data.backoff.micros, 100000);
		assert_eq!(config.plugin_data.max_spawn.attempts, 3);
		assert_eq!(config.plugin_data.max_conn.attempts, 5);
		assert_eq!(config.plugin_data.jitter.percent, 10);
		assert_eq!(config.plugin_data.grpc_buffer.size, 10);
	}
	
}
