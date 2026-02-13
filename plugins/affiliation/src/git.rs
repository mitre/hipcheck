// SPDX-License-Identifier: Apache-2.0

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
	collections::HashMap,
	fmt::{self, Display, Formatter},
	result::Result as StdResult,
};

/// Commits as understood in Hipcheck's data model.
/// The `written_on` and `committed_on` datetime fields contain Strings that are created from `jiff:Timestamps`.
/// Because `Timestamp` does not `impl JsonSchema`, we display the datetimes as Strings for passing out of this plugin.
/// Other plugins that expect a `Timestamp`` should parse the provided Strings into `Timestamps` as needed.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, JsonSchema)]
pub struct Commit {
	pub hash: String,

	pub written_on: StdResult<String, String>,

	pub committed_on: StdResult<String, String>,
}

impl Display for Commit {
	fn fmt(&self, f: &mut Formatter) -> fmt::Result {
		write!(f, "{}", self.hash)
	}
}

/// Authors or committers of a commit.
#[derive(
	Debug, PartialEq, Eq, Deserialize, Serialize, Clone, Hash, PartialOrd, Ord, JsonSchema,
)]
pub struct Contributor {
	pub name: String,
	pub email: String,
}

impl Display for Contributor {
	fn fmt(&self, f: &mut Formatter) -> fmt::Result {
		write!(f, "{} <{}>", self.name, self.email)
	}
}

/// Temporary data structure for looking up the contributors of a commit
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq, JsonSchema)]
pub struct CommitContributorView {
	pub commit: Commit,
	pub author: Contributor,
	pub committer: Contributor,
}

impl CommitContributorView {
	/// get the author of the commit
	pub fn author(&self) -> &Contributor {
		&self.author
	}

	/// get the committer of the commit
	pub fn committer(&self) -> &Contributor {
		&self.committer
	}
}

/// struct for counting number of contributions made by each contributor in the repo
pub struct ContributorFrequencyMap<'a>(HashMap<&'a Contributor, u64>);

impl<'a> ContributorFrequencyMap<'a> {
	pub fn new(capacity: Option<usize>) -> Self {
		match capacity {
			Some(capacity) => Self(HashMap::with_capacity(capacity)),
			None => Self(HashMap::new()),
		}
	}

	/// Add an affiliated_contributor to the map, update frequency for affiliated_contributor
	/// appropriately
	///
	/// If the contributor is not in the HashMap, then insert and set frequency to 1
	/// Otherwise, increment frequency by 1
	pub fn insert(&mut self, affiliated_contributor: &'a Contributor) {
		self.0
			.entry(affiliated_contributor)
			.and_modify(|count| *count += 1)
			.or_insert(1);
	}

	/// Retrieve a non-mutable handle to the internal HashMap
	pub fn inner(&self) -> &HashMap<&'a Contributor, u64> {
		&self.0
	}
}
