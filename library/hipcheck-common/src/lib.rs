// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;

use std::{result::Result as StdResult, str::FromStr};

pub mod chunk;
pub mod error;
pub mod types;

pub mod proto {
	include!(concat!(env!("OUT_DIR"), "/hipcheck.v1.rs"));
}

pub struct QueryTarget {
	pub publisher: String,
	pub plugin: String,
	pub query: Option<String>,
}

impl FromStr for QueryTarget {
	type Err = anyhow::Error;

	fn from_str(s: &str) -> StdResult<Self, Self::Err> {
		let parts: Vec<&str> = s.split('/').collect();
		match parts.as_slice() {
			[publisher, plugin, query] => Ok(Self {
				publisher: publisher.to_string(),
				plugin: plugin.to_string(),
				query: Some(query.to_string()),
			}),
			[publisher, plugin] => Ok(Self {
				publisher: publisher.to_string(),
				plugin: plugin.to_string(),
				query: None,
			}),
			_ => Err(anyhow!("Invalid query target string '{}'", s)),
		}
	}
}

impl TryInto<QueryTarget> for &str {
	type Error = anyhow::Error;
	fn try_into(self) -> StdResult<QueryTarget, Self::Error> {
		QueryTarget::from_str(self)
	}
}
