// SPDX-License-Identifier: Apache-2.0

//! `radish` provides a single macro: `hash`, which returns the hashed value of some collection of values.

/// Hashes a collection of hashable values together and returns the result.
#[macro_export]
macro_rules! hash {
    ( $( $part:expr ),* ) => {{
		use std::collections::hash_map::DefaultHasher;
		use std::hash::{Hash, Hasher};

		let mut hasher = DefaultHasher::new();
		$(
			$part.hash(&mut hasher);
		)*

		hasher.finish()
    }};

    ($( $part:expr, )*) => ($crate::hash![$($part),*])
}

#[cfg(test)]
mod tests {
	use crate::hash;
	use std::collections::hash_map::DefaultHasher;
	use std::hash::{Hash, Hasher};

	#[test]
	fn it_works() {
		let h = hash!(5);

		let expected = {
			let mut hasher = DefaultHasher::new();
			5.hash(&mut hasher);
			hasher.finish()
		};

		assert_eq!(h, expected);
	}
}
