// SPDX-License-Identifier: Apache-2.0

use crate::{context::Context as _, error::Result, metric::MetricProvider};
use chrono::{prelude::*, Duration};
use serde::{ser::SerializeStruct, Serialize, Serializer};
use std::{result::Result as StdResult, sync::Arc};

#[derive(Debug, Eq, PartialEq)]
pub struct ActivityOutput {
	pub today: DateTime<FixedOffset>,
	pub last_commit_date: DateTime<FixedOffset>,
	pub time_since_last_commit: Duration,
}

impl Serialize for ActivityOutput {
	fn serialize<S>(&self, serializer: S) -> StdResult<S::Ok, S::Error>
	where
		S: Serializer,
	{
		let midnight = NaiveTime::from_hms_opt(0, 0, 0).unwrap();
		let today = self.today.with_time(midnight).unwrap();
		let last_commit_date = self.last_commit_date.with_time(midnight).unwrap();
		let time_since_last_commit = self.time_since_last_commit.to_string();

		let mut state = serializer.serialize_struct("Output", 3)?;

		state.serialize_field("today", &today)?;
		state.serialize_field("last_commit_date", &last_commit_date)?;
		state.serialize_field("time_since_last_commit", &time_since_last_commit)?;

		state.end()
	}
}

pub(crate) fn activity_metric(db: &dyn MetricProvider) -> Result<Arc<ActivityOutput>> {
	log::debug!("running activity metric");

	// Get today's date.
	let today = utc_to_fixed_offset(Utc::now());

	// Get the date of the most recent commit.
	let last_commit_date = db
		.last_commit_date()
		.context("failed to get last commit date for activity metric")?;

	// Get the time between the most recent commit and today.
	let time_since_last_commit = today - last_commit_date;

	log::info!("completed activity metric");

	Ok(Arc::new(ActivityOutput {
		today,
		last_commit_date,
		time_since_last_commit,
	}))
}

fn utc_to_fixed_offset(date: DateTime<Utc>) -> DateTime<FixedOffset> {
	let offset = date.timezone().fix();
	DateTime::from_naive_utc_and_offset(date.naive_utc(), offset)
}
