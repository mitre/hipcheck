// SPDX-License-Identifier: Apache-2.0

use crate::context::Context as _;
use crate::data::Fuzz;
use crate::error::Result;
use crate::metric::MetricProvider;
use serde::Serialize;
use std::sync::Arc;

#[derive(Debug, Eq, PartialEq, Serialize)]
pub struct FuzzOutput {
	pub fuzz_result: Fuzz,
}

pub fn fuzz_metric(db: &dyn MetricProvider) -> Result<Arc<FuzzOutput>> {
	log::debug!("running fuzz metric");

	let fuzz_result = db
		.fuzz_check()
		.context("failed to get response from fuzz check")?;

	log::info!("completed fuzz metric");

	Ok(Arc::new(FuzzOutput { fuzz_result }))
}
