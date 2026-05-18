// SPDX-License-Identifier: Apache-2.0

/// Pairs a query endpoint name with a particular `Query` trait implementation.
///
/// Since the `Query` trait needs to be made into a trait object, we can't use a static associated
/// string to store the query's name in the trait itself. This object wraps a `Query` trait object
/// and allows us to associate a name with it so that when the plugin receives a query from
/// Hipcheck core, it can look up the proper behavior to invoke.
pub struct QueryEndpoint {
	/// The name of the query.
	pub name: &'static str,

	/// The `Query` trait object.
	pub inner: crate::query::DynQuery,
}

impl QueryEndpoint {
	/// Returns whether the current query is the plugin's default query, determined by whether the
	/// query name is empty.
	pub(crate) fn is_default(&self) -> bool {
		self.name.is_empty()
	}
}
