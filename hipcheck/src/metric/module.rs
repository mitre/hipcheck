// SPDX-License-Identifier: Apache-2.0

use crate::{
	data::ModuleGraph,
	error::{Context as _, Result},
	metric::MetricProvider,
};
use serde::Serialize;
use std::sync::Arc;

#[derive(Debug, Eq, PartialEq, Serialize)]
pub struct ModuleOutput {
	pub module_graph: Arc<ModuleGraph>,
	pub is_modular: bool,
}

pub fn module_analysis(db: &dyn MetricProvider) -> Result<Arc<ModuleOutput>> {
	log::debug!("running module analysis");

	let module_graph = db
		.get_module_graph()
		.context("failed to get module graph")?;

	log::trace!("got module graph [pr='{:#?}']", module_graph);

	let modules = ModuleOutput {
		module_graph,
		is_modular: true,
	};

	log::info!("completed module analysis");

	Ok(Arc::new(modules))
}
