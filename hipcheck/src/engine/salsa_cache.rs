// SPDX-License-Identifier: Apache-2.0

use crate::{
	engine::{HcEngine, RUNTIME},
	hc_error,
	plugin::{get_plugin_key, PluginResponse, QueryResult},
	Result,
};

trait CloneFromSalsaDb {
	/// the type being wrapped with a newtype for use with salsa
	type Inner;

	/// extract the value held within a `#[salsa::input]` newtype wrapper
	fn clone_from_salsa_db(&self, engine: &dyn HcEngine) -> Self::Inner;
}

/// Salsa wants newtype wrappers with `#[salsa::input]` added to any arguments passed
/// to `#[salsa::tracked]` functions.
///
/// This macro generates the required code needed for any argument passed to a
/// `#[salsa::tracked]` function.
///
/// NOTE: `#[salsa::input]` creates a `::new(inner: $newtype) -> Self` function already so there is
/// no need to generate one here
macro_rules! salsa_query_newtype {
	($struct_name:ident, $newtype:ty) => {
		#[salsa::input]
		pub struct $struct_name {
			inner: $newtype,
		}

		impl CloneFromSalsaDb for $struct_name {
			type Inner = $newtype;

			fn clone_from_salsa_db(&self, engine: &dyn HcEngine) -> Self::Inner {
				self.inner(engine)
			}
		}
	};
}

salsa_query_newtype!(SalsaPublisher, String);
salsa_query_newtype!(SalsaPlugin, String);
salsa_query_newtype!(SalsaQuery, String);
salsa_query_newtype!(SalsaKey, serde_json::Value);

// Salsa doesn't natively support async functions, so our recursive `query()` function that
// interacts with plugins (which use async) has to get a handle to the underlying runtime,
// spawn and block on a task to query the plugin, then choose whether to recurse or return.

#[salsa::tracked]
/// query the plugin system and utilize salsa caching whenever possible
pub fn query_with_salsa(
	engine: &dyn HcEngine,
	publisher: SalsaPublisher,
	plugin: SalsaPlugin,
	query: SalsaQuery,
	key: SalsaKey,
) -> Result<QueryResult> {
	let hash_key = get_plugin_key(
		publisher.clone_from_salsa_db(engine).as_str(),
		plugin.clone_from_salsa_db(engine).as_str(),
	);

	#[cfg(feature = "print-timings")]
	let _0 = crate::benchmarking::print_scope_time!(format!("{}/{}", &hash_key, &query_value));

	let runtime = RUNTIME.handle();
	let core = engine.core();

	// Find the plugin
	let Some(p_handle) = core.plugins.get(&hash_key) else {
		return Err(hc_error!("No such plugin {}", hash_key));
	};
	// Initiate the query. If remote closed or we got our response immediately,
	// return
	let mut ar = match runtime.block_on(p_handle.query(
		query.clone_from_salsa_db(engine),
		key.clone_from_salsa_db(engine),
	))? {
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
			let value = engine
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
