mod download_manifest;
mod kdl_parsing;
mod manager;
mod plugin_manifest;
mod retrieval;
mod supported_arch;
mod types;

pub use crate::plugin::manager::*;
pub use crate::plugin::types::*;
use crate::policy_exprs::Expr;
use crate::Result;
pub use download_manifest::{
	ArchiveFormat, DownloadManifest, DownloadManifestEntry, HashAlgorithm, HashWithDigest,
};
use futures::future::join_all;
pub use kdl_parsing::{extract_data, ParseKdlNode};
pub use plugin_manifest::{PluginManifest, PluginName, PluginPublisher, PluginVersion};
use serde_json::Value;
use std::collections::HashMap;
pub use supported_arch::SupportedArch;
use tokio::sync::{mpsc, Mutex};

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

pub async fn initialize_plugins(
	plugins: Vec<PluginContextWithConfig>,
) -> Result<Vec<PluginTransport>> {
	let mut set = tokio::task::JoinSet::new();
	for (p, c) in plugins
		.into_iter()
		.map(Into::<(PluginContext, Value)>::into)
	{
		set.spawn(p.initialize(c));
	}
	let mut out: Vec<PluginTransport> = vec![];
	while let Some(res) = set.join_next().await {
		out.push(res??);
	}
	Ok(out)
}

#[derive(Debug)]
pub struct ActivePlugin {
	next_id: Mutex<usize>,
	channel: PluginTransport,
}
impl ActivePlugin {
	pub fn new(channel: PluginTransport) -> Self {
		ActivePlugin {
			next_id: Mutex::new(1),
			channel,
		}
	}
	pub fn get_default_policy_expr(&self) -> Option<&Expr> {
		self.channel.opt_default_policy_expr.as_ref()
	}
	async fn get_unique_id(&self) -> usize {
		let mut id_lock = self.next_id.lock().await;
		let res: usize = *id_lock;
		// even IDs reserved for plugin-originated queries, so skip to next odd ID
		*id_lock += 2;
		drop(id_lock);
		res
	}
	pub async fn query(&self, name: String, key: Value) -> Result<PluginResponse> {
		let id = self.get_unique_id().await;
		// @Todo - check name+key valid for schema
		let query = Query {
			id,
			request: true,
			publisher: "".to_owned(),
			plugin: self.channel.name().to_owned(),
			query: name,
			key,
			output: serde_json::json!(null),
		};
		Ok(self.channel.query(query).await?.into())
	}
	pub async fn resume_query(
		&self,
		state: AwaitingResult,
		output: Value,
	) -> Result<PluginResponse> {
		let query = Query {
			id: state.id,
			request: false,
			publisher: state.publisher,
			plugin: state.plugin,
			query: state.query,
			key: serde_json::json!(null),
			output,
		};
		eprintln!("Resuming query with answer {query:?}");
		Ok(self.channel.query(query).await?.into())
	}
}

#[derive(Debug)]
pub struct HcPluginCore {
	executor: PluginExecutor,
	pub plugins: HashMap<String, ActivePlugin>,
}
impl HcPluginCore {
	// When this object is returned, the plugins are all connected but the
	// initialization protocol over the gRPC still needs to be completed
	pub async fn new(executor: PluginExecutor, plugins: Vec<(PluginWithConfig)>) -> Result<Self> {
		// Separate plugins and configs so we can start plugins async
		let mut conf_map = HashMap::<String, Value>::new();
		let plugins = plugins
			.into_iter()
			.map(|pc| {
				let (p, c) = pc.into();
				conf_map.insert(p.name.clone(), c);
				p
			})
			.collect();
		let ctxs = executor.start_plugins(plugins).await?;
		// Rejoin plugin ctx with its config
		let mapped_ctxs: Vec<PluginContextWithConfig> = ctxs
			.into_iter()
			.map(|c| {
				let conf = conf_map.remove(&c.plugin.name).unwrap();
				PluginContextWithConfig(c, conf)
			})
			.collect();
		// Use configs to initialize corresponding plugin
		let plugins = HashMap::<String, ActivePlugin>::from_iter(
			initialize_plugins(mapped_ctxs)
				.await?
				.into_iter()
				.map(|p| (p.name().to_owned(), ActivePlugin::new(p))),
		);
		// Now we have a set of started and initialized plugins to interact with
		Ok(HcPluginCore { executor, plugins })
	}
}
