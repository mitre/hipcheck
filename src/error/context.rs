// SPDX-License-Identifier: Apache-2.0

//! A duplicate of the `anyhow::Context` extension trait intended to
//! make error propagation less verbose.

use crate::error::{Error, Introspect};
use std::error::Error as StdError;

/// Functions for adding context to an error result
///
/// The `Context` trait is based around the `Error` type defined in
/// this crate.  Aside from the changed method names (collision
/// avoidance), it is a duplicate of the `anyhow::Context` trait.
/// Like its `anyhow` counterpart, this trait is sealed.
pub trait Context<T>: sealed::Sealed {
	/// Add context to an error
	fn context<C>(self, context: C) -> Result<T, Error>
	where
		C: Introspect + 'static;

	/// Lazily add context to an error
	fn with_context<C, F>(self, context_fn: F) -> Result<T, Error>
	where
		C: Introspect + 'static,
		F: FnOnce() -> C;
}

// `Context` is implemented only for those result types encountered
// when entering or traversing the query system: `Result<T, Error>`
// and `Result<T, E>` for dynamic error types `E`.

impl<T> Context<T> for Result<T, Error> {
	fn context<C>(self, context: C) -> Result<T, Error>
	where
		C: Introspect + 'static,
	{
		self.map_err(|err| err.context(context))
	}

	fn with_context<C, F>(self, context_fn: F) -> Result<T, Error>
	where
		C: Introspect + 'static,
		F: FnOnce() -> C,
	{
		self.map_err(|err| err.context(context_fn()))
	}
}

impl<T, E> Context<T> for Result<T, E>
where
	E: StdError + Send + Sync + 'static,
{
	fn context<C>(self, context: C) -> Result<T, Error>
	where
		C: Introspect + 'static,
	{
		self.map_err(|err| Error::from(err).context(context))
	}

	fn with_context<C, F>(self, context_fn: F) -> Result<T, Error>
	where
		C: Introspect + 'static,
		F: FnOnce() -> C,
	{
		self.map_err(|err| Error::from(err).context(context_fn()))
	}
}

// Restricts implementations of `Context` only to those contained in
// this module
mod sealed {
	use super::{Error, StdError};

	pub trait Sealed {}

	impl<T> Sealed for Result<T, Error> {}

	impl<T, E> Sealed for Result<T, E> where E: StdError + 'static {}
}

#[cfg(test)]
mod tests {
	//! Tests to ensure `Context` produces output correctly.

	use crate::error::Error;
	use std::{io, io::ErrorKind};

	// Message source root error with no context
	#[test]
	fn debug_behavior_msg_no_context() {
		let error = Error::msg("error message");
		let debug = format!("{:?}", error);
		let expected = "error message".to_string();
		assert_eq!(expected, debug);
	}

	// Message source root error with a single context message
	#[test]
	fn debug_behavior_msg_single_context() {
		let error = Error::msg("error message").context("context");
		let debug = format!("{:?}", error);
		let expected = "context\n\nCaused by: \n    0: error message".to_string();
		assert_eq!(expected, debug);
	}

	// Message source root error with multiple context messages
	#[test]
	fn debug_behavior_msg_multiple_context() {
		let error = Error::msg("error message")
			.context("context 1")
			.context("context 2");
		let debug = format!("{:?}", error);
		let expected =
			"context 2\n\nCaused by: \n    0: context 1\n    1: error message".to_string();
		assert_eq!(expected, debug);
	}

	// Dynamic error source with no context
	#[test]
	fn debug_behavior_std_no_context() {
		let error = Error::from(io::Error::new(
			ErrorKind::ConnectionRefused,
			"connection refused",
		));

		let debug = format!("{:?}", error);
		let expected = "connection refused".to_string();
		assert_eq!(expected, debug);
	}

	// Dynamic error source with a single context message
	#[test]
	fn debug_behavior_std_single_context() {
		let error = Error::from(io::Error::new(
			ErrorKind::ConnectionRefused,
			"connection refused",
		))
		.context("context");

		let debug = format!("{:?}", error);
		let expected = "context\n\nCaused by: \n    0: connection refused".to_string();
		assert_eq!(expected, debug);
	}

	// Dynamic error source with multiple context messages
	#[test]
	fn debug_behavior_std_multiple_context() {
		let error = Error::from(io::Error::new(
			ErrorKind::ConnectionRefused,
			"connection refused",
		))
		.context("context 1")
		.context("context 2");

		let debug = format!("{:?}", error);
		let expected =
			"context 2\n\nCaused by: \n    0: context 1\n    1: connection refused".to_string();
		assert_eq!(expected, debug);
	}
}
