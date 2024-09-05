#![allow(unused)]

use crate::analysis::{
	score::{
		ACTIVITY_PHASE, AFFILIATION_PHASE, BINARY_PHASE, CHURN_PHASE, ENTROPY_PHASE, FUZZ_PHASE,
		IDENTITY_PHASE, REVIEW_PHASE, TYPO_PHASE,
	},
	AnalysisProvider,
};
use crate::metric::{review::PullReview, MetricProvider};
use crate::plugin::{ActivePlugin, PluginResponse};
pub use crate::plugin::{HcPluginCore, PluginExecutor, PluginWithConfig};
use crate::policy_exprs::Expr;
use crate::{hc_error, Result};
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

	fn default_policy_expr(&self, publisher: String, plugin: String) -> Result<Option<String>>;

	fn query(&self, publisher: String, plugin: String, query: String, key: Value) -> Result<Value>;
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
) -> Result<Value> {
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
		let answer = db.query(
			ar.publisher.clone(),
			ar.plugin.clone(),
			ar.query.clone(),
			ar.key.clone(),
		)?;
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
) -> BoxFuture<'static, Result<Value>> {
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
			.await?;
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
	pub fn new(executor: PluginExecutor, plugins: Vec<(PluginWithConfig)>) -> Result<Self> {
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
