// SPDX-License-Identifier: Apache-2.0

pub use crate::plugin::{HcPluginCore, PluginExecutor, PluginWithConfig};
use crate::{
	cache::plugin::HcPluginCache,
	hc_error,
	plugin::{
		get_current_arch, get_plugin_key, retrieve_plugins, Plugin, PluginManifest, PluginResponse,
		QueryResult,
	},
	policy::PolicyFile,
	policy_exprs::Expr,
	Result,
};
use futures::future::{BoxFuture, FutureExt};
use serde_json::Value;
use std::sync::{Arc, LazyLock};
use tokio::runtime::{Handle, Runtime};

// Salsa doesn't natively support async functions, so our recursive `query()` function that
// interacts with plugins (which use async) has to get a handle to the underlying runtime,
// spawn and block on a task to query the plugin, then choose whether to recurse or return.

static RUNTIME: LazyLock<Runtime> = LazyLock::new(|| Runtime::new().unwrap());

#[salsa::query_group(HcEngineStorage)]
pub trait HcEngine: salsa::Database {
	#[salsa::input]
	fn core(&self) -> Arc<HcPluginCore>;

	fn default_policy_expr(&self, publisher: String, plugin: String) -> Result<Option<Expr>>;

	fn default_query_explanation(
		&self,
		publisher: String,
		plugin: String,
	) -> Result<Option<String>>;

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
) -> Result<Option<Expr>> {
	let core = db.core();
	let key = get_plugin_key(publisher.as_str(), plugin.as_str());
	let Some(p_handle) = core.plugins.get(&key) else {
		return Err(hc_error!("No such plugin {}", key));
	};
	Ok(p_handle.get_default_policy_expr().cloned())
}

fn default_query_explanation(
	db: &dyn HcEngine,
	publisher: String,
	plugin: String,
) -> Result<Option<String>> {
	let core = db.core();
	let key = get_plugin_key(publisher.as_str(), plugin.as_str());
	let Some(p_handle) = core.plugins.get(&key) else {
		return Err(hc_error!("Plugin '{}' not found", key,));
	};
	Ok(p_handle.get_default_query_explanation().cloned())
}

fn query(
	db: &dyn HcEngine,
	publisher: String,
	plugin: String,
	query: String,
	key: Value,
) -> Result<QueryResult> {
	let hash_key = get_plugin_key(publisher.as_str(), plugin.as_str());

	#[cfg(feature = "print-timings")]
	let _0 = crate::benchmarking::print_scope_time!(format!("{}/{}", &hash_key, &query));

	let runtime = RUNTIME.handle();
	let core = db.core();

	// Find the plugin
	let Some(p_handle) = core.plugins.get(&hash_key) else {
		return Err(hc_error!("No such plugin {}", hash_key));
	};
	// Initiate the query. If remote closed or we got our response immediately,
	// return
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
		log::trace!("Query needs more info, recursing...");
		let mut answers = vec![];

		// per RFD 0009, each key will be used to query `salsa` independently
		for key in ar.key.clone() {
			// since one key is used to query `salsa`, there will only be one value returned and
			// the `pop().unwrap() is safe`
			let value = db
				.query(
					ar.publisher.clone(),
					ar.plugin.clone(),
					ar.query.clone(),
					key,
				)?
				.value
				.pop()
				.unwrap();
			answers.push(value);
		}
		log::trace!("Got answer, resuming");
		ar = match runtime.block_on(p_handle.resume_query(ar, answers))? {
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
		let hash_key = get_plugin_key(publisher.as_str(), plugin.as_str());

		#[cfg(feature = "print-timings")]
		let _0 = crate::benchmarking::print_scope_time!(format!("{}/{}", &hash_key, &query));

		// Find the plugin
		let Some(p_handle) = core.plugins.get(&hash_key) else {
			return Err(hc_error!("No such plugin {}", hash_key));
		};
		// Initiate the query. If remote closed or we got our response immediately,
		// return
		log::trace!("Querying: {query}, key: {key:?}");
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
			log::trace!("Awaiting result, now recursing");
			let mut answers = vec![];
			// per RFD 0009, each key will be used to query `salsa` independently
			for key in ar.key.clone() {
				// since one key is used to query `salsa`, there will only be one value returned and
				// the `pop().unwrap() is safe`
				let value = async_query(
					Arc::clone(&core),
					ar.publisher.clone(),
					ar.plugin.clone(),
					ar.query.clone(),
					key,
				)
				.await?
				.value
				.pop()
				.unwrap();
				answers.push(value);
			}
			log::trace!("Resuming query with answers {:#?}", answers);
			ar = match p_handle.resume_query(ar, answers).await? {
				PluginResponse::RemoteClosed => {
					return Err(hc_error!("Plugin channel closed unexpected"));
				}
				PluginResponse::Completed(v) => {
					return Ok(v);
				}
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
		log::info!("Starting HcPluginCore");
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
	executor: PluginExecutor,
) -> Result<Arc<HcPluginCore>> {
	let current_arch = get_current_arch();

	// retrieve, verify and extract all required plugins
	let required_plugin_names = retrieve_plugins(&policy_file.plugins.0, plugin_cache)?;

	let mut plugins = vec![];
	for plugin_id in required_plugin_names.iter() {
		let plugin_kdl = plugin_cache.plugin_kdl(plugin_id);
		let working_dir = plugin_kdl
			.parent()
			.expect("The plugin.kdl is always in the plugin cache")
			.to_owned();
		let plugin_manifest = PluginManifest::from_file(plugin_kdl)?;
		let entrypoint = plugin_manifest
			.get_entrypoint(&current_arch)
			.ok_or_else(|| {
				hc_error!(
					"Could not find {} entrypoint for {}",
					current_arch,
					plugin_id
				)
			})?;

		let plugin = Plugin {
			name: plugin_id.to_policy_file_plugin_identifier(),
			version: plugin_id.version().clone(),
			working_dir,
			entrypoint,
		};

		// find and serialize config for plugin
		let config = policy_file
			.get_config(plugin_id.to_policy_file_plugin_identifier().as_str())
			.ok_or_else(|| hc_error!("Could not find config for {}", plugin_id))?;
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
