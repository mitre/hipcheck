// SPDX-License-Identifier: Apache-2.0

//! Utility type for hiding data from the user when printed in a debug message.

use std::fmt::{Debug, Formatter, Result as FmtResult};

/// Helper container to ensure a value isn't printed.
pub struct Redacted<T>(T);

impl<T> Redacted<T> {
	/// Construct a new redacted value.
	pub fn new(val: T) -> Redacted<T> {
		Redacted(val)
	}
}

impl<T> AsRef<T> for Redacted<T> {
	fn as_ref(&self) -> &T {
		&self.0
	}
}

impl<T> AsMut<T> for Redacted<T> {
	fn as_mut(&mut self) -> &mut T {
		&mut self.0
	}
}

impl<T> Debug for Redacted<T> {
	fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
		write!(f, "<redacted>")
	}
}
