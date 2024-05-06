// SPDX-License-Identifier: Apache-2.0

use crate::analysis::MetricProvider;
use crate::context::Context as _;
use crate::data::Fuzz;
use crate::error::Result;
use serde::Serialize;
use serde::{self};
use std::rc::Rc;

#[derive(Debug, Eq, PartialEq, Serialize)]
#[serde(crate = "self::serde")]
pub struct FuzzOutput {
	pub fuzz_result: Fuzz,
}

pub fn fuzz_metric(db: &dyn MetricProvider) -> Result<Rc<FuzzOutput>> {
	log::debug!("running fuzz metric");

	let fuzz_result = db
		.fuzz_check()
		.context("failed to get response from fuzz check")?;

	log::info!("completed fuzz metric");

	Ok(Rc::new(FuzzOutput { fuzz_result }))
}
