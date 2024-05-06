// SPDX-License-Identifier: Apache-2.0

use crate::MetricProvider;
use hc_common::context::Context as _;
use hc_common::{data::ModuleGraph, error::Result, log};
use serde::{self, Serialize};
use std::rc::Rc;

#[derive(Debug, Eq, PartialEq, Serialize)]
#[serde(crate = "self::serde")]
pub struct ModuleOutput {
	pub module_graph: Rc<ModuleGraph>,
	pub is_modular: bool,
}

pub fn module_analysis(db: &dyn MetricProvider) -> Result<Rc<ModuleOutput>> {
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

	Ok(Rc::new(modules))
}
