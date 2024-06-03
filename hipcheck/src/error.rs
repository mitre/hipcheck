// SPDX-License-Identifier: Apache-2.0

//! An error type suitable for use in Hipcheck's query system.
//!
//! Salsa requires memoized query-value types to implement `Clone` and
//! `Eq`. The `anyhow::Error` type implements neither, making it
//! difficult to work with directly in this setting.
//!
//! Instead, the `Error` type defined in this crate ensures queries
//! which error out aren't retried, as it always compares as equal to
//! any other error.

use std::borrow::Cow;
use std::error::Error as StdError;
use std::fmt;
use std::fmt::Debug;
use std::fmt::Display;
use std::rc::Rc;

pub type Result<T> = std::result::Result<T, Error>;

/// A type convertible into a `Cow<'static, str>`.
///
/// This impl ensures we can avoid allocations for all of the static string
/// error messages which exist in the Hipcheck source code.
pub trait Introspect: Into<Cow<'static, str>> {}
impl<T: Into<Cow<'static, str>>> Introspect for T {}

/// An error type compatible with Salsa.
pub struct Error {
	/// The start of the error linked list.
	head: Rc<ErrorNode>,
}

impl Error {
	/// Create a new `Error` with a message source.
	pub fn msg<S>(message: S) -> Self
	where
		S: Into<Cow<'static, str>>,
	{
		let error = Message(message.into());
		Error::new(error)
	}

	/// Create a new `Error` from a source error.
	pub fn new<M>(error: M) -> Self
	where
		M: StdError + 'static,
	{
		Error {
			head: Rc::new(ErrorNode {
				current: Rc::new(error),
				next: None,
			}),
		}
	}

	/// Add additional context to an `Error`
	pub(crate) fn context<M>(self, context: M) -> Self
	where
		M: Introspect + 'static,
	{
		let message: Cow<'static, str> = context.into();

		log::trace!(
			"adding context to error [context: {}, error: {}]",
			message,
			self.head
		);

		Error {
			head: Rc::new(ErrorNode {
				current: Rc::new(Message(message)),
				next: Some(self.head),
			}),
		}
	}

	/// Get an iterator over the errors in a chain.
	pub fn chain(&self) -> Chain {
		Chain::new(self)
	}
}

/// Allows use of `?` operator on query system entry.
impl<T> From<T> for Error
where
	T: StdError + 'static,
{
	fn from(std_error: T) -> Error {
		Error::new(std_error)
	}
}

impl Clone for Error {
	fn clone(&self) -> Error {
		Error {
			head: Rc::clone(&self.head),
		}
	}
}

// By defining all `Error` instances to be equal, the query system
// will not update a value with further errors after reaching an
// initial one.
impl PartialEq for Error {
	fn eq(&self, _: &Self) -> bool {
		true
	}
}

impl Eq for Error {}

impl Debug for Error {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		// Delegate to the debug impl for the head of the list.
		Debug::fmt(self.head.as_ref(), f)
	}
}

impl Display for Error {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		// Delegate to the display impl for the head of the list.
		Display::fmt(self.head.as_ref(), f)
	}
}

/// A single node in the linked list of errors.
pub struct ErrorNode {
	/// The current error.
	current: ErrorObj,
	/// A next error, if present.
	next: Option<ErrorLink>,
}

impl Debug for ErrorNode {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{}", self.current)?;

		if self.next.is_some() {
			write!(f, "\n\nCaused by: ")?;

			let mut index = 0;
			let mut link = self.next.as_ref();

			while let Some(step) = link {
				write!(f, "\n{:5}: {}", index, step.current)?;
				link = step.next.as_ref();
				index += 1;
			}

			match (index, link) {
				// Only printed one message.
				(0, Some(step)) => write!(f, "\n    {}", step.current)?,
				// Printed more than one.
				(_, Some(step)) => write!(f, "\n{:5}: {}", index, step.current)?,
				// Nothing to print.
				(_, None) => {}
			}
		}

		Ok(())
	}
}

impl Display for ErrorNode {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{}", self.current)
	}
}

impl StdError for ErrorNode {
	fn source(&self) -> Option<&(dyn StdError + 'static)> {
		self.next
			.as_deref()
			.map(|node| node as &(dyn StdError + 'static))
	}
}

/// A reference-counted fat pointer to a standard error type.
type ErrorObj = Rc<dyn StdError + 'static>;

/// A link in the linked list.
type ErrorLink = Rc<ErrorNode>;

/// A string-only error message, which can either be a static string
/// slice, or an owned string.
#[derive(Debug)]
struct Message(Cow<'static, str>);

impl Display for Message {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{}", self.0)
	}
}

impl StdError for Message {
	fn source(&self) -> Option<&(dyn StdError + 'static)> {
		None
	}
}

pub struct Chain<'e> {
	current: Option<&'e ErrorNode>,
}

impl<'e> Chain<'e> {
	fn new(error: &Error) -> Chain<'_> {
		Chain {
			current: Some(error.head.as_ref()),
		}
	}
}

impl<'e> Iterator for Chain<'e> {
	type Item = &'e ErrorNode;

	fn next(&mut self) -> Option<Self::Item> {
		match self.current {
			Some(node) => {
				log::trace!("error in chain [error: {}]", node);

				self.current = node.next.as_deref();
				Some(node)
			}
			None => None,
		}
	}
}

/// A limited analogue of the `anyhow!` macro for `Error`.  Only
/// intended for input suitable for the `Error::msg` function.
#[macro_export]
macro_rules! hc_error {
    ($msg:literal $(,)?) => {
        $crate::error::Error::msg($msg)
    };
    ($fmt:expr, $($arg:tt)*) => {
        $crate::error::Error::msg(format!($fmt, $($arg)*))
    };
}

#[cfg(test)]
mod tests {
	//! Tests to ensure `Error` produces output correctly.

	// Literal input to `hc_error`
	#[test]
	fn macro_literal() {
		let error = hc_error!("msg source");
		let debug = format!("{:?}", error);
		let expected = "msg source".to_string();
		assert_eq!(expected, debug);
	}

	// Format string input to `hc_error`
	#[test]
	fn macro_format_string() {
		let msg = "msg";
		let source = "source";
		let error = hc_error!("format {} {}", msg, source);
		let debug = format!("{:?}", error);
		let expected = "format msg source".to_string();
		assert_eq!(expected, debug);
	}

	// Verify that the `chain` method on `hc_error` works.
	#[test]
	fn hc_error_chain() {
		let error = hc_error!("first error");
		let error = error.context("second error");
		let error = error.context("third error");

		let mut iter = error.chain();

		assert_eq!("third error", iter.next().unwrap().to_string());
		assert_eq!("second error", iter.next().unwrap().to_string());
		assert_eq!("first error", iter.next().unwrap().to_string());
	}
}
