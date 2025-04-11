// SPDX-License-Identifier: Apache-2.0

mod arch;
mod download_manifest;
mod manager;
mod plugin_id;
mod plugin_manifest;
mod retrieval;
mod types;

pub use crate::plugin::{
	get_plugin_key,
	manager::*,
	plugin_id::{PluginId, PluginIdVersionRange},
	types::*,
};
use crate::policy_exprs::Expr;
use crate::{error::Result, hc_error};
pub use arch::{get_current_arch, try_set_arch, Arch, KnownArch};
pub use download_manifest::{ArchiveFormat, DownloadManifest, HashAlgorithm, HashWithDigest};
use hipcheck_common::types::{Query, QueryDirection};
pub(crate) use plugin_manifest::validate_version_req;
pub use plugin_manifest::{
	try_get_bin_for_entrypoint, PluginManifest, PluginName, PluginPublisher, PluginVersion,
	PluginVersionReq,
};
pub use retrieval::retrieve_plugins;
use serde_json::Value;
use std::fmt::Write as _;
use std::{collections::HashMap, ops::Not};
use tokio::sync::Mutex;

pub async fn initialize_plugins(
	plugins: Vec<PluginContextWithConfig>,
) -> Result<Vec<PluginTransport>> {
	let mut set = tokio::task::JoinSet::new();

	for (p, c) in plugins
		.into_iter()
		.map(Into::<(PluginContext, Value)>::into)
	{
		set.spawn(p.init_return_plugin(c));
	}

	let mut inited: Vec<PluginTransport> = vec![];
	let mut failures: Vec<_> = vec![];

	while let Some(res) = set.join_next().await {
		// @Todo - what is the cleanup if the tokio func fails?
		let (plugin, init_res) = res?;

		// Instead of immediately erroring, we need to finish
		// initializing all plugins so they shut down properly
		match init_res {
			Ok(pt) => inited.push(pt),
			Err(e) => failures.push((plugin, e)),
		};
	}

	if failures.is_empty().not() {
		let mut plugin_errors = "Failures occurred during plugin initialization:".to_owned();
		for (plugin, err) in failures {
			write!(
				plugin_errors,
				"\nPlugin '{}:{}': {}",
				plugin.name, plugin.version.version, err
			)?;
		}
		Err(hc_error!("{}", plugin_errors))
	} else {
		Ok(inited)
	}
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
			direction: QueryDirection::Request,
			publisher: publisher.to_owned(),
			plugin: plugin.to_owned(),
			query: name,
			key: vec![key],
			output: vec![],
			concerns: vec![],
		};

		Ok(self.channel.query(query).await?.into())
	}

	pub async fn resume_query(
		&self,
		state: AwaitingResult,
		output: Vec<Value>,
	) -> Result<PluginResponse> {
		let query = Query {
			id: state.id,
			direction: QueryDirection::Response,
			publisher: state.publisher,
			plugin: state.plugin,
			query: state.query,
			key: vec![],
			output,
			concerns: vec![],
		};

		log::trace!("Resuming query");

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
				let conf = conf_map.get(&c.plugin.name).cloned().unwrap();
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
