#![allow(unused)]

use crate::plugin::{ActivePlugin, HcPluginCore, PluginExecutor, PluginResponse, PluginWithConfig};
use crate::{hc_error, Result};
use serde_json::Value;
use std::sync::{Arc, LazyLock};
use tokio::runtime::Runtime;

// Salsa doesn't natively support async functions, so our recursive `query()` function that
// interacts with plugins (which use async) has to get a handle to the underlying runtime,
// spawn and block on a task to query the plugin, then choose whether to recurse or return.

static RUNTIME: LazyLock<Runtime> = LazyLock::new(|| Runtime::new().unwrap());

#[salsa::query_group(HcEngineStorage)]
pub trait HcEngine: salsa::Database {
	#[salsa::input]
	fn core(&self) -> Arc<HcPluginCore>;

	fn query(&self, publisher: String, plugin: String, query: String, key: Value) -> Result<Value>;
}

fn query(
	db: &dyn HcEngine,
	publisher: String,
	plugin: String,
	query: String,
	key: Value,
) -> Result<Value> {
	let runtime = RUNTIME.handle();
	let core = db.core();
	// Find the plugin
	let Some(p_handle) = core.plugins.get(&plugin) else {
		return Err(hc_error!("No such plugin {}::{}", publisher, plugin));
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
		let answer = db.query(
			ar.publisher.clone(),
			ar.plugin.clone(),
			ar.query.clone(),
			ar.key.clone(),
		)?;
		ar = match runtime.block_on(p_handle.resume_query(ar, answer))? {
			PluginResponse::RemoteClosed => {
				return Err(hc_error!("Plugin channel closed unexpected"));
			}
			PluginResponse::Completed(v) => return Ok(v),
			PluginResponse::AwaitingResult(a) => a,
		};
	}
}

#[salsa::database(HcEngineStorage)]
pub struct HcEngineImpl {
	// Query storage
	storage: salsa::Storage<Self>,
}

impl salsa::Database for HcEngineImpl {}

impl HcEngineImpl {
	// Really HcEngineImpl and HcPluginCore do the same thing right now, except HcPluginCore
	// has an async constructor. If we can manipulate salsa to accept async functions, we
	// could consider merging the two structs. Although maybe its wise to keep HcPluginCore
	// independent of Salsa.
	pub fn new(executor: PluginExecutor, plugins: Vec<(PluginWithConfig)>) -> Result<Self> {
		let runtime = RUNTIME.handle();
		let core = runtime.block_on(HcPluginCore::new(executor, plugins))?;
		let mut engine = HcEngineImpl {
			storage: Default::default(),
		};
		engine.set_core(Arc::new(core));
		Ok(engine)
	}
	// TODO - "run" function that takes analysis heirarchy and target, and queries each
	// analysis plugin to kick off the execution
}
