mod manager;
mod parser;
mod types;

use crate::plugin::manager::*;
pub use crate::plugin::types::*;

pub fn dummy() {
	let plugin = Plugin {
		name: "dummy".to_owned(),
		entrypoint: "./dummy".to_owned(),
	};
	let manager = PluginExecutor::new(
		/* max_spawn_attempts */ 3,
		/* max_conn_attempts */ 5,
		/* port_range */ 40000..u16::MAX,
		/* backoff_interval_micros */ 1000,
		/* jitter_percent */ 10,
	);
}
