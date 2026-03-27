// SPDX-License-Identifier: Apache-2.0

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
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
pub struct GitIdentity {
	pub name: String,
	pub email: String,
}

impl Display for GitIdentity {
	fn fmt(&self, f: &mut Formatter) -> fmt::Result {
		write!(f, "{} <{}>", self.name, self.email)
	}
}

/// Temporary data structure for looking up the contributors of a commit
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq, JsonSchema)]
pub struct GitCommitContributors {
	pub commit: Commit,
	pub author: GitIdentity,
	pub committer: GitIdentity,
}

impl GitCommitContributors {
	/// get the author of the commit
	pub fn author(&self) -> &GitIdentity {
		&self.author
	}

	/// get the committer of the commit
	pub fn committer(&self) -> &GitIdentity {
		&self.committer
	}
}
