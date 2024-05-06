use std::fmt::Debug;
use std::fmt::Formatter;
use std::fmt::Result as FmtResult;

/// Helper container to ensure a value isn't printed.
pub struct Hidden<T>(T);

impl<T> Hidden<T> {
	/// Construct a new hidden value.
	pub fn new(val: T) -> Hidden<T> {
		Hidden(val)
	}
}

impl<T> AsRef<T> for Hidden<T> {
	fn as_ref(&self) -> &T {
		&self.0
	}
}

impl<T> AsMut<T> for Hidden<T> {
	fn as_mut(&mut self) -> &mut T {
		&mut self.0
	}
}

impl<T> Debug for Hidden<T> {
	fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
		write!(f, "<redacted>")
	}
}
