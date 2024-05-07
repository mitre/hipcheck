// SPDX-License-Identifier: Apache-2.0

//! A trait containing a fallible analogue of the `Iterator::any`
//! method.

/// A fallible analogue of the `Iterator::any` method
pub trait TryAny: Iterator {
	fn try_any<F, E>(&mut self, mut f: F) -> Result<bool, E>
	where
		F: FnMut(<Self as Iterator>::Item) -> Result<bool, E>,
	{
		for t in self {
			match f(t) {
				Ok(false) => continue,
				result => return result,
			}
		}

		Ok(false)
	}
}

impl<I: Iterator> TryAny for I {}

#[cfg(test)]
mod tests {
	use super::*;

	fn odd_not_three(n: usize) -> Result<bool, String> {
		if n == 3 {
			Err(String::from("Error"))
		} else if n % 2 == 1 {
			Ok(true)
		} else {
			Ok(false)
		}
	}

	#[test]
	fn any_ok_true() {
		let v = vec![2, 4, 5];

		let result = v.into_iter().try_any(odd_not_three).unwrap();

		assert!(result);
	}

	#[test]
	fn any_ok_true_with_three() {
		let v = vec![2, 4, 5, 3];

		let result = v.into_iter().try_any(odd_not_three).unwrap();

		assert!(result);
	}

	#[test]
	fn any_ok_false() {
		let v = vec![2, 4, 6, 8, 10];

		let result = v.into_iter().try_any(odd_not_three).unwrap();

		assert!(!result);
	}

	#[test]
	#[should_panic]
	fn any_err() {
		let v = vec![2, 4, 3, 1, 5, 7, 9];

		v.into_iter().try_any(odd_not_three).unwrap();
	}
}
