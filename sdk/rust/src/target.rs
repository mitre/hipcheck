use crate::error::Error;
use std::{result::Result as StdResult, str::FromStr};

/// Identifies the target plugin and endpoint of a Hipcheck query.
///
/// The `publisher` and `plugin` fields are necessary from Hipcheck core's perspective to identify
/// a plugin process. Plugins may define one or more query endpoints, and may include an unnamed
/// endpoint as the "default", hence why the `query` field is optional. `QueryTarget` implements
/// `FromStr` so it can be parsed from strings of the format `"publisher/plugin[/query]"`, where
/// the bracketed substring is optional.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QueryTarget {
	pub publisher: String,
	pub plugin: String,
	pub query: Option<String>,
}

impl FromStr for QueryTarget {
	type Err = Error;

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
			_ => Err(Error::InvalidQueryTargetFormat),
		}
	}
}
impl std::fmt::Display for QueryTarget {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match &self.query {
			Some(query) => write!(f, "{}/{}/{}", self.publisher, self.plugin, query),
			None => write!(f, "{}/{}", self.publisher, self.plugin),
		}
	}
}

impl TryInto<QueryTarget> for &str {
	type Error = Error;
	fn try_into(self) -> StdResult<QueryTarget, Self::Error> {
		QueryTarget::from_str(self)
	}
}
