// SPDX-License-Identifier: Apache-2.0

mod arch;
mod download_manifest;
mod manager;
mod plugin_id;
mod plugin_manifest;
mod retrieval;
mod types;

use crate::error::Result;
pub use crate::plugin::{get_plugin_key, manager::*, plugin_id::PluginId, types::*};
pub use arch::{get_current_arch, try_set_arch, Arch};
pub use download_manifest::{ArchiveFormat, DownloadManifest, HashAlgorithm, HashWithDigest};
pub use plugin_manifest::{PluginManifest, PluginName, PluginPublisher, PluginVersion};
pub use retrieval::{retrieve_plugins, MITRE_LEGACY_PLUGINS};
use serde_json::Value;
use std::collections::HashMap;
use tokio::sync::Mutex;

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

	pub fn get_default_policy_expr(&self) -> Option<&String> {
		self.channel.opt_default_policy_expr.as_ref()
	}

	pub fn get_default_query_explanation(&self) -> Option<&String> {
		self.channel.opt_explain_default_query.as_ref()
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

		// TODO: remove this unwrap
		let (publisher, plugin) = self.channel.name().split_once('/').unwrap();

		// @Todo - check name+key valid for schema
		let query = Query {
			id,
			request: true,
			publisher: publisher.to_owned(),
			plugin: plugin.to_owned(),
			query: name,
			key,
			output: serde_json::json!(null),
			concerns: vec![],
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
			concerns: vec![],
		};

		eprintln!("Resuming query with answer {query:?}");

		Ok(self.channel.query(query).await?.into())
	}
}

#[derive(Debug)]
pub struct HcPluginCore {
	pub plugins: HashMap<String, ActivePlugin>,
}

impl HcPluginCore {
	// When this object is returned, the plugins are all connected but the
	// initialization protocol over the gRPC still needs to be completed
	pub async fn new(executor: PluginExecutor, plugins: Vec<PluginWithConfig>) -> Result<Self> {
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
		Ok(HcPluginCore { plugins })
	}
}
