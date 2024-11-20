// SPDX-License-Identifier: Apache-2.0

pub mod score;

use crate::{
	config::{AttacksConfigQuery, CommitConfigQuery, PracticesConfigQuery},
	data::git::GitProvider,
	error::Result,
	metric::MetricProvider,
	plugin::QueryResult,
	F64,
};
use std::{
	collections::HashSet,
	default::Default,
};

/// Queries about analyses
#[salsa::query_group(AnalysisProviderStorage)]
pub trait AnalysisProvider:
	AttacksConfigQuery + CommitConfigQuery + GitProvider + MetricProvider + PracticesConfigQuery
{

	/// Returns result of identity analysis
	fn identity_analysis(&self) -> Result<QueryResult>;

	/// Returns result of fuzz analysis
	fn fuzz_analysis(&self) -> Result<QueryResult>;

}

  

pub fn identity_analysis(db: &dyn AnalysisProvider) -> Result<QueryResult> {
	let results = db.identity_metric()?;
	let num_flagged = results
		.matches
		.iter()
		.filter(|m| m.identities_match)
		.count() as u64;
	let percent_flagged = num_flagged as f64 / results.matches.len() as f64;
	let value = F64::new(percent_flagged).expect("Percent threshold should never be NaN");
	Ok(QueryResult {
		value: serde_json::to_value(value)?,
		concerns: vec![],
	})
}

pub fn fuzz_analysis(db: &dyn AnalysisProvider) -> Result<QueryResult> {
	let results = db.fuzz_metric()?;
	let value = results.fuzz_result.exists;
	Ok(QueryResult {
		value: serde_json::to_value(value)?,
		concerns: vec![],
	})
}