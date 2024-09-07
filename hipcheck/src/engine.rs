// SPDX-License-Identifier: Apache-2.0

pub use crate::plugin::{HcPluginCore, PluginExecutor, PluginWithConfig};
use crate::{
	cache::plugin_cache::HcPluginCache,
	hc_error,
	plugin::{retrieve_plugins, Plugin, PluginManifest, PluginResponse, QueryResult, CURRENT_ARCH},
	policy::PolicyFile,
	util::fs::{find_file_by_name, read_string},
	Result,
};
use futures::future::{BoxFuture, FutureExt};
use serde_json::Value;
use std::{
	str::FromStr,
	sync::{Arc, LazyLock},
};
use tokio::runtime::{Handle, Runtime};

// Salsa doesn't natively support async functions, so our recursive `query()` function that
// interacts with plugins (which use async) has to get a handle to the underlying runtime,
// spawn and block on a task to query the plugin, then choose whether to recurse or return.

static RUNTIME: LazyLock<Runtime> = LazyLock::new(|| Runtime::new().unwrap());

#[salsa::query_group(HcEngineStorage)]
pub trait HcEngine: salsa::Database {
	#[salsa::input]
	fn core(&self) -> Arc<HcPluginCore>;

	fn default_policy_expr(&self, publisher: String, plugin: String) -> Result<Option<String>>;

	fn query(
		&self,
		publisher: String,
		plugin: String,
		query: String,
		key: Value,
	) -> Result<QueryResult>;
}

fn default_policy_expr(
	db: &dyn HcEngine,
	publisher: String,
	plugin: String,
) -> Result<Option<String>> {
	let core = db.core();
	// @Todo - plugins map should be keyed on publisher too
	let Some(p_handle) = core.plugins.get(&plugin) else {
		return Err(hc_error!("No such plugin {}::{}", publisher, plugin));
	};
	Ok(p_handle.get_default_policy_expr().cloned())
}

fn query(
	db: &dyn HcEngine,
	publisher: String,
	plugin: String,
	query: String,
	key: Value,
) -> Result<QueryResult> {
	let runtime = RUNTIME.handle();
	let core = db.core();
	// Find the plugin
	let Some(p_handle) = core.plugins.get(&plugin) else {
		return Err(hc_error!("No such plugin {}::{}", publisher, plugin));
	};
	// Initiate the query. If remote closed or we got our response immediately,
	// return
	println!("Querying {plugin}::{query} with key {key:?}");
	let mut ar = match runtime.block_on(p_handle.query(query, key))? {
		PluginResponse::RemoteClosed => {
			return Err(hc_error!("Plugin channel closed unexpected"));
		}
		PluginResponse::Completed(v) => return Ok(v),
		PluginResponse::AwaitingResult(a) => a,
	};
	// Otherwise, the plugin needs more data to continue. Recursively query
	// (with salsa memo-ization) to get the needed data, and resume our
	// current query by providing the plugin the answer.
	loop {
		println!("Query needs more info, recursing...");
		let answer = db
			.query(
				ar.publisher.clone(),
				ar.plugin.clone(),
				ar.query.clone(),
				ar.key.clone(),
			)?
			.value;
		println!("Got answer {answer:?}, resuming");
		ar = match runtime.block_on(p_handle.resume_query(ar, answer))? {
			PluginResponse::RemoteClosed => {
				return Err(hc_error!("Plugin channel closed unexpected"));
			}
			PluginResponse::Completed(v) => return Ok(v),
			PluginResponse::AwaitingResult(a) => a,
		};
	}
}

// Demonstration of how the above `query()` function would be implemented as async
pub fn async_query(
	core: Arc<HcPluginCore>,
	publisher: String,
	plugin: String,
	query: String,
	key: Value,
) -> BoxFuture<'static, Result<QueryResult>> {
	async move {
		// Find the plugin
		let Some(p_handle) = core.plugins.get(&plugin) else {
			return Err(hc_error!("No such plugin {}::{}", publisher, plugin));
		};
		// Initiate the query. If remote closed or we got our response immediately,
		// return
		println!("Querying: {query}, key: {key:?}");
		let mut ar = match p_handle.query(query, key).await? {
			PluginResponse::RemoteClosed => {
				return Err(hc_error!("Plugin channel closed unexpected"));
			}
			PluginResponse::Completed(v) => {
				return Ok(v);
			}
			PluginResponse::AwaitingResult(a) => a,
		};
		// Otherwise, the plugin needs more data to continue. Recursively query
		// (with salsa memo-ization) to get the needed data, and resume our
		// current query by providing the plugin the answer.
		loop {
			println!("Awaiting result, now recursing");
			let answer = async_query(
				Arc::clone(&core),
				ar.publisher.clone(),
				ar.plugin.clone(),
				ar.query.clone(),
				ar.key.clone(),
			)
			.await?
			.value;
			println!("Resuming query with answer {answer:?}");
			ar = match p_handle.resume_query(ar, answer).await? {
				PluginResponse::RemoteClosed => {
					return Err(hc_error!("Plugin channel closed unexpected"));
				}
				PluginResponse::Completed(v) => return Ok(v),
				PluginResponse::AwaitingResult(a) => a,
			};
		}
	}
	.boxed()
}

#[salsa::database(HcEngineStorage)]
pub struct HcEngineImpl {
	// Query storage
	storage: salsa::Storage<Self>,
}

impl salsa::Database for HcEngineImpl {}

impl salsa::ParallelDatabase for HcEngineImpl {
	fn snapshot(&self) -> salsa::Snapshot<Self> {
		salsa::Snapshot::new(HcEngineImpl {
			storage: self.storage.snapshot(),
		})
	}
}

impl HcEngineImpl {
	// Really HcEngineImpl and HcPluginCore do the same thing right now, except HcPluginCore
	// has an async constructor. If we can manipulate salsa to accept async functions, we
	// could consider merging the two structs. Although maybe its wise to keep HcPluginCore
	// independent of Salsa.
	pub fn new(executor: PluginExecutor, plugins: Vec<PluginWithConfig>) -> Result<Self> {
		let runtime = RUNTIME.handle();
		println!("Starting HcPluginCore");
		let core = runtime.block_on(HcPluginCore::new(executor, plugins))?;
		let mut engine = HcEngineImpl {
			storage: Default::default(),
		};
		engine.set_core(Arc::new(core));
		Ok(engine)
	}
	pub fn runtime() -> &'static Handle {
		RUNTIME.handle()
	}
	// TODO - "run" function that takes analysis heirarchy and target, and queries each
	// analysis plugin to kick off the execution
}

pub fn start_plugins(
	policy_file: &PolicyFile,
	plugin_cache: &HcPluginCache,
) -> Result<Arc<HcPluginCore>> {
	let executor = PluginExecutor::new(
		/* max_spawn_attempts */ 3,
		/* max_conn_attempts */ 5,
		/* port_range */ 40000..u16::MAX,
		/* backoff_interval_micros */ 1000,
		/* jitter_percent */ 10,
	)?;

	// retrieve, verify and extract all required plugins
	let required_plugin_names = retrieve_plugins(&policy_file.plugins.0, plugin_cache)?;

	let mut plugins = vec![];
	for plugin_id in required_plugin_names.iter() {
		let plugin_dir = plugin_cache.plugin_download_dir(
			&plugin_id.publisher,
			&plugin_id.name,
			&plugin_id.version,
		);

		// determine entrypoint for this plugin
		let plugin_kdl = find_file_by_name(plugin_dir, "plugin.kdl")?;
		let contents = read_string(&plugin_kdl)?;
		let plugin_manifest = PluginManifest::from_str(contents.as_str())?;
		let entrypoint = plugin_manifest
			.get_entrypoint(CURRENT_ARCH)
			.ok_or_else(|| {
				hc_error!(
					"Could not find {} entrypoint for {}/{} {}",
					CURRENT_ARCH,
					plugin_id.publisher.0,
					plugin_id.name.0,
					plugin_id.version.0
				)
			})?;

		let plugin = Plugin {
			name: plugin_id.name.0.clone(),
			entrypoint,
		};

		// find and serialize config for plugin
		let config = policy_file
			.get_config(plugin_id.to_policy_file_plugin_identifier().as_str())
			.ok_or_else(|| {
				hc_error!(
					"Could not find config for {} {}",
					plugin_id.to_policy_file_plugin_identifier(),
					plugin_id.version.0
				)
			})?;
		let config = serde_json::to_value(&config).map_err(|_e| {
			hc_error!(
				"Error serializing config for {}",
				plugin_id.to_policy_file_plugin_identifier()
			)
		})?;

		let plugin_with_config = PluginWithConfig(plugin, config);
		plugins.push(plugin_with_config);
	}

	let runtime = RUNTIME.handle();
	let core = runtime.block_on(HcPluginCore::new(executor, plugins))?;
	Ok(Arc::new(core))
}
