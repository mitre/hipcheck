use crate::hipcheck::plugin_client::PluginClient;
use crate::plugin::{HcPluginClient, Plugin, PluginContext};
use crate::{hc_error, Result, F64};
use futures::future::join_all;
use futures::Future;
use rand::Rng;
use std::collections::HashSet;
use std::ops::Range;
use std::process::{Command, Stdio};
use tokio::time::{sleep_until, Duration, Instant};

#[derive(Clone, Debug)]
pub struct PluginExecutor {
	max_spawn_attempts: usize,
	max_conn_attempts: usize,
	port_range: Range<u16>,
	backoff_interval: Duration,
	jitter_percent: u8,
}
impl PluginExecutor {
	pub fn new(
		max_spawn_attempts: usize,
		max_conn_attempts: usize,
		port_range: Range<u16>,
		backoff_interval_micros: u64,
		jitter_percent: u8,
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
		})
	}
	fn get_available_port(&self) -> Result<u16> {
		for i in self.port_range.start..self.port_range.end {
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
	pub async fn start_plugins(&self, plugins: Vec<Plugin>) -> Result<Vec<PluginContext>> {
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
		eprintln!("Starting plugin '{}'", plugin.name);
		let mut spawn_attempts: usize = 0;
		while spawn_attempts < self.max_spawn_attempts {
			// Find free port for process. Don't retry if we fail since this means all
			// ports in the desired range are already bound
			let port = self.get_available_port()?;
			let port_str = port.to_string();
			// Spawn plugin process
			eprintln!("Spawning '{}' on port {}", &plugin.entrypoint, port_str);
			let Ok(mut proc) = Command::new(&plugin.entrypoint)
				.args(["--port", port_str.as_str()])
				// @Temporary - directly forward stdout/stderr from plugin to shell
				.stdout(std::io::stdout())
				.stderr(std::io::stderr())
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
				let jitter: i32 = rand::thread_rng().gen_range(0..(2 * self.jitter_percent)) as i32;
				// Then subtract by self.jitter_percent to center around 0, and add to 100%
				let jitter_percent = 1.0 + ((jitter - (self.jitter_percent as i32)) as f64 / 100.0);
				// Once we are confident this math works, we can remove this
				if !(0.0..=2.0).contains(&jitter_percent) {
					panic!("Math error! We should have better guardrails around PluginExecutor field values.");
				}
				// sleep_duration = (backoff * conn_attempts) * (1.0 +/- jitter_percent)
				let mut sleep_duration: Duration = self
					.backoff_interval
					.saturating_mul(conn_attempts as u32)
					.mul_f64(jitter_percent);
				sleep_until(Instant::now() + sleep_duration).await;
				if let Ok(grpc) =
					PluginClient::connect(format!("http://127.0.0.1:{port_str}")).await
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
				if let Err(e) = proc.kill() {
					println!("Failed to kill child process for plugin: {e}");
				}
				spawn_attempts += 1;
				continue;
			};
			// We now have an open gRPC connection to our plugin process
			return Ok(PluginContext {
				plugin: plugin.clone(),
				port,
				grpc,
				proc,
			});
		}
		Err(hc_error!(
			"Reached max spawn attempts for plugin {}",
			plugin.name
		))
	}
}
