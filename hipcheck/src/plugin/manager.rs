// SPDX-License-Identifier: Apache-2.0

use crate::{
	hc_error,
	plugin::{try_get_bin_for_entrypoint, HcPluginClient, Plugin, PluginContext},
	policy::policy_file::PolicyPluginName,
	Result,
};
use futures::future::join_all;
use hipcheck_common::{proto::plugin_service_client::PluginServiceClient, types::LogLevel};
use rand::Rng;
use serde::Deserialize;
use std::{collections::HashMap, env, ffi::OsString, ops::Range, path::Path, sync::LazyLock};
use tokio::process::Command;
use tokio::{
	io::{AsyncBufReadExt, BufReader},
	time::{sleep_until, Duration, Instant},
};

static PLUGIN_LOG_SETTINGS: LazyLock<PluginLogLevels> =
	LazyLock::new(PluginLogLevels::new_from_env);

/// structs for deserializing plugin logs
#[derive(Debug, Deserialize)]
struct LogEvent {
	level: LogLevel,
	fields: LogFields,
	target: String,
}

#[derive(Debug, Deserialize)]
struct LogFields {
	message: String,
}

pub struct PluginLogLevels {
	plugin_wide_level: LogLevel,
	plugin_level_map: HashMap<String, LogLevel>,
}
impl PluginLogLevels {
	/// Parses HC_LOG env var to fetch log level for each plugin as Struct
	pub fn new_from_env() -> Self {
		// default level to error when var unset
		let mut plugin_wide_level = LogLevel::Error;
		let mut plugin_level_map: HashMap<String, LogLevel> = HashMap::new();

		let Ok(rust_log) = env::var("HC_LOG") else {
			return PluginLogLevels {
				plugin_wide_level,
				plugin_level_map,
			};
		};

		let settings: Vec<&str> = rust_log.split(',').collect();
		let mut plugin_wide_set = false;

		for setting in settings {
			let parts: Vec<&str> = setting.split('=').collect();

			match (parts.first(), parts.get(1)) {
				(Some(module), Some(level)) => {
					let log_level = match LogLevel::level_from_str(level) {
						Ok(level) => level,
						Err(e) => {
							tracing::error!("{}", e);
							continue;
						}
					};

					if *module == "plugin" {
						// e.g. HC_LOG="plugin=info"
						// plugin wide log level is set, overriding global level
						plugin_wide_level = log_level;
						plugin_wide_set = true;
					} else {
						// e.g. HC_LOG="plugin::typo=info"
						// plugin specific log level is found
						plugin_level_map.insert(module.to_string(), log_level);
					}
				}
				(Some(level), _) => {
					// e.g. HC_LOG="info"
					let log_level = match LogLevel::level_from_str(level) {
						Ok(level) => level,
						Err(e) => {
							tracing::error!("{}", e);
							continue;
						}
					};
					// Global log level used for plugins, if no plugin wide found
					if !plugin_wide_set {
						plugin_wide_level = log_level;
					}
				}
				(None, _) => tracing::error!("Incomplete HC_LOG var set"),
			}
		}
		PluginLogLevels {
			plugin_wide_level,
			plugin_level_map,
		}
	}

	/// Retrieves the log level for a given plugin, using most specific level available
	fn get_log_level_for_module(&self, plugin_name: &str) -> &LogLevel {
		// use plugin specific
		if let Some(level) = self.plugin_level_map.get(plugin_name) {
			level
		} else {
			// use plugin wide
			&self.plugin_wide_level
		}
	}
}

#[derive(Clone, Debug)]
pub struct PluginExecutor {
	max_spawn_attempts: usize,
	max_conn_attempts: usize,
	port_range: Range<u16>,
	backoff_interval: Duration,
	jitter_percent: u8,
	grpc_buffer: usize,
}
impl PluginExecutor {
	pub fn new(
		max_spawn_attempts: usize,
		max_conn_attempts: usize,
		port_range: Range<u16>,
		backoff_interval_micros: u64,
		jitter_percent: u8,
		grpc_buffer: usize,
	) -> Result<Self> {
		if jitter_percent > 100 {
			return Err(hc_error!(
				"jitter_percent must be <= 100, got {}",
				jitter_percent
			));
		}

		let backoff_interval = Duration::from_micros(backoff_interval_micros);

		Ok(PluginExecutor {
			max_spawn_attempts,
			max_conn_attempts,
			port_range,
			backoff_interval,
			jitter_percent,
			grpc_buffer,
		})
	}

	fn get_available_port(&self) -> Result<u16> {
		for _i in self.port_range.start..self.port_range.end {
			// @Todo - either TcpListener::bind returns Ok even if port is bound
			// or we have a race condition. For now just have OS assign a port
			// if std::net::TcpListener::bind(format!("127.0.0.1:{i}")).is_ok() {
			// 	return Ok(i);
			// }
			if let Ok(addr) = std::net::TcpListener::bind("127.0.0.1:0") {
				if let Ok(local_addr) = addr.local_addr() {
					return Ok(local_addr.port());
				}
			}
		}

		Err(hc_error!("Failed to find available port"))
	}

	/// Launches async forwarding of plugin logs to program output.
	fn forward_plugin_logs(
		&self,
		reader: impl tokio::io::AsyncRead + Unpin + Send + 'static,
		plugin_title: String,
	) {
		tokio::spawn(async move {
			let reader = BufReader::new(reader);
			let mut lines = reader.lines();
			// parse plugin log json lines to LogEvent objects and output the messages

			// create new tracing_subscriber with no target
			loop {
				match lines.next_line().await {
					Ok(Some(line)) => {
						if let Ok(event) = serde_json::from_str::<LogEvent>(&line) {
							let message = event.fields.message;
							let event_target = match event.target.contains("hipcheck_sdk") {
								true => {
									format!("plugin::{}::{}", plugin_title, event.target)
								}
								false => format!("plugin::{}", event.target),
							};
							match event.level {
								// use custom field instead of target?

								// create target statically from plugin_title? is that possible?
								LogLevel::Error => tracing::error!(target: &event_target, "{}", message),
								LogLevel::Warn => tracing::warn!(plugin_target = &event_target, "{}", message),
								LogLevel::Info => tracing::info!(plugin_target = &event_target, "{}", message),
								LogLevel::Debug => tracing::debug!(plugin_target = &event_target, "{}", message),
								LogLevel::Trace => tracing::trace!(plugin_target = &event_target, "{}", message),
								LogLevel::Off => {}
							};
						}
						// drop plugin logs without message
					}
					Ok(None) => {
						break;
					}
					Err(e) => {
						tracing::error!(
							"I/O erorr occured while listening for logs from {plugin_title}: {e}"
						);
						break;
					}
				};
			}
		});
	}

	pub async fn start_plugins(&self, plugins: Vec<Plugin>) -> Result<Vec<PluginContext>> {
		let num_plugins = plugins.len();
		tracing::info!("Starting {} plugins", num_plugins);

		join_all(plugins.into_iter().map(|p| self.start_plugin(p)))
			.await
			.into_iter()
			.collect()
	}

	pub async fn start_plugin(&self, plugin: Plugin) -> Result<PluginContext> {
		// Plugin startup design has inherent TOCTOU flaws since we tell the plugin
		// which port we expect it to bind to. We can try to ensure the port we pass
		// on the cmdline is not already in use, but it is still possible for that
		// port to become unavailable between our check and the plugin's bind attempt.
		// Hence the need for subsequent attempts if we get unlucky

		// `entrypoint` is a string that represents a CLI invocation which may contain
		// arguments, so we have to split off only the first token
		let (opt_bin_path_str, args) = try_get_bin_for_entrypoint(&plugin.entrypoint);
		let Some(bin_path_str) = opt_bin_path_str else {
			return Err(hc_error!(
				"Unable to get bin path for plugin entrypoint '{}'",
				&plugin.entrypoint
			));
		};

		let Ok(canon_working_dir) = plugin.working_dir.canonicalize() else {
			return Err(hc_error!(
				"Failed to canonicalize plugin working dir: {:?}",
				&plugin.working_dir
			));
		};

		// Entrypoints are often "<BIN_NAME>" which can overlap with existing binaries on the
		// system like "git", "npm", so we must search for <BIN_NAME> from within the plugin
		// cache subfolder. First, grab the existing path.
		let Some(mut which_os_paths) =
			std::env::var_os("PATH").map(|s| std::env::split_paths(&s).collect::<Vec<_>>())
		else {
			return Err(hc_error!("Unable to get system PATH var"));
		};

		// Add canonicalized plugin cache dir to end of PATH for plugin exec
		let mut cmd_os_paths = which_os_paths.clone();
		cmd_os_paths.push(canon_working_dir.clone());
		let cmd_path = std::env::join_paths(cmd_os_paths).unwrap();

		// Add canonicalized plugin cache dir to front of PATH for bin-searching
		which_os_paths.insert(0, canon_working_dir.clone());
		let which_path = std::env::join_paths(which_os_paths).unwrap();

		// Find the binary_str using temp PATH
		let Ok(canon_bin_path) = which::which_in::<&str, &OsString, &Path>(
			bin_path_str,
			Some(&which_path),
			canon_working_dir.as_ref(),
		) else {
			return Err(hc_error!(
				"Failed to find binary '{}' for plugin",
				bin_path_str
			));
		};

		tracing::debug!(
			"Starting plugin '{}' at '{:?}'",
			plugin.name,
			&canon_bin_path
		);

		let mut spawn_attempts: usize = 0;
		while spawn_attempts < self.max_spawn_attempts {
			let mut spawn_args = args.clone();

			// Find free port for process. Don't retry if we fail since this means all
			// ports in the desired range are already bound
			let port = self.get_available_port()?;
			let port_str = port.to_string();
			spawn_args.push("--port");
			spawn_args.push(port_str.as_str());

			// pass logging level for plugin
			let plugin_title = PolicyPluginName::new(&plugin.name)
				.expect("plugin name")
				.name
				.0;
			let plugin_path = format!("plugin::{}", plugin_title);
			let plugin_log_level = PLUGIN_LOG_SETTINGS.get_log_level_for_module(&plugin_path);
			let plugin_log_level_str = plugin_log_level.to_string();
			spawn_args.push("--log-level");
			spawn_args.push(&plugin_log_level_str);

			// Spawn plugin process
			tracing::debug!("Spawning '{}' on port {}", &plugin.entrypoint, port_str);
			let Ok(mut proc) = Command::new(&canon_bin_path)
				.env("PATH", &cmd_path)
				.args(spawn_args)
				// pipe output to tokio task for plugin log forwarding
				.stdout(std::process::Stdio::piped())
				.stderr(std::process::Stdio::piped())
				.kill_on_drop(true)
				.spawn()
			else {
				spawn_attempts += 1;
				continue;
			};
			// Attempt to connect to the plugin's gRPC server up to N times, using
			// linear backoff with a percentage jitter.
			let mut conn_attempts = 0;
			let mut opt_grpc: Option<HcPluginClient> = None;
			while conn_attempts < self.max_conn_attempts {
				// Jitter could be positive or negative, so mult by 2 to cover both sides
				let jitter: i32 = rand::rng().random_range(0..(2 * self.jitter_percent)) as i32;
				// Then subtract by self.jitter_percent to center around 0, and add to 100%
				let jitter_percent = 1.0 + ((jitter - (self.jitter_percent as i32)) as f64 / 100.0);
				// Once we are confident this math works, we can remove this
				if !(0.0..=2.0).contains(&jitter_percent) {
					panic!("Math error! We should have better guardrails around PluginExecutor field values.");
				}
				// sleep_duration = (backoff * conn_attempts) * (1.0 +/- jitter_percent)
				let sleep_duration: Duration = self
					.backoff_interval
					.saturating_mul(conn_attempts as u32)
					.mul_f64(jitter_percent);
				sleep_until(Instant::now() + sleep_duration).await;
				if let Ok(grpc) =
					PluginServiceClient::connect(format!("http://127.0.0.1:{port_str}")).await
				{
					opt_grpc = Some(grpc);
					break;
				} else {
					conn_attempts += 1;
				}
			}
			// If opt_grpc is None, we did not manage to connect to the plugin. Kill it
			// and try again
			let Some(grpc) = opt_grpc else {
				if let Err(e) = proc.kill().await {
					tracing::error!("Failed to kill child process for plugin: {e}");
				}
				spawn_attempts += 1;
				continue;
			};

			let plugin_stdout = proc.stdout.take().expect("Failed to capture plugin stdout");
			let plugin_stderr = proc.stderr.take().expect("Failed to capture plugin stderr");

			self.forward_plugin_logs(plugin_stdout, plugin_title.clone());
			self.forward_plugin_logs(plugin_stderr, plugin_title.clone());

			// We now have an open gRPC connection to our plugin process
			return Ok(PluginContext {
				plugin: plugin.clone(),
				port,
				grpc,
				proc,
				grpc_query_buffer_size: self.grpc_buffer,
			});
		}
		Err(hc_error!(
			"Reached max spawn attempts for plugin {}",
			plugin.name
		))
	}
}

/// Test Plugin log level parsing
#[cfg(test)]
mod tests {
	use hipcheck_common::types::LogLevel;

	use super::PluginLogLevels;
	use crate::util::test::with_env_vars;

	#[test]
	fn plugin_wide_log_level() {
		let vars = vec![("HC_LOG", Some("plugin::git=trace,plugin=debug,off"))];

		with_env_vars(vars, || {
			let plugin_log_settings = PluginLogLevels::new_from_env();

			assert_eq!(
				&LogLevel::Debug,
				plugin_log_settings.get_log_level_for_module("plugin::affiliation")
			);
			assert_eq!(
				&LogLevel::Trace,
				plugin_log_settings.get_log_level_for_module("plugin::git")
			);
			assert_eq!(
				&LogLevel::Debug,
				plugin_log_settings.get_log_level_for_module("typo")
			);
		});
	}

	#[test]
	fn plugin_specific_log_level() {
		let vars = vec![("HC_LOG", Some("plugin::affiliation=info,plugin::git=trace"))];

		with_env_vars(vars, || {
			let plugin_log_settings = PluginLogLevels::new_from_env();

			assert_eq!(
				&LogLevel::Info,
				plugin_log_settings.get_log_level_for_module("plugin::affiliation")
			);
			assert_eq!(
				&LogLevel::Trace,
				plugin_log_settings.get_log_level_for_module("plugin::git")
			);
			assert_eq!(
				&LogLevel::Error,
				plugin_log_settings.get_log_level_for_module("typo")
			);
		});
	}
}
