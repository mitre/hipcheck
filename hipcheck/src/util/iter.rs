// SPDX-License-Identifier: Apache-2.0

//! Iterator extension traits.

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

/// Represents an iterator and a fallible criterion for filtering it
pub struct FallibleFilter<I, P, E>
where
	I: Iterator,
	P: FnMut(&<I as Iterator>::Item) -> Result<bool, E>,
{
	iterator: I,
	predicate: P,
}

impl<I, P, E> FallibleFilter<I, P, E>
where
	I: Iterator,
	P: FnMut(&<I as Iterator>::Item) -> Result<bool, E>,
{
	fn new(iterator: I, predicate: P) -> Self {
		FallibleFilter {
			iterator,
			predicate,
		}
	}
}

impl<I, P, E> Iterator for FallibleFilter<I, P, E>
where
	I: Iterator,
	P: FnMut(&<I as Iterator>::Item) -> Result<bool, E>,
{
	type Item = Result<<I as Iterator>::Item, E>;

	fn next(&mut self) -> Option<Self::Item> {
		if let Some(t) = self.iterator.next() {
			match (self.predicate)(&t) {
				Ok(true) => Some(Ok(t)),
				Ok(false) => self.next(),
				Err(e) => Some(Err(e)),
			}
		} else {
			None
		}
	}
}

/// Apply a fallible filter to an Iterator, returning the elements
/// selected by the filter and any errors that occur.
pub trait TryFilter: Sized + Iterator {
	fn try_filter<P, E>(self, predicate: P) -> FallibleFilter<Self, P, E>
	where
		P: FnMut(&<Self as Iterator>::Item) -> Result<bool, E>,
	{
		FallibleFilter::new(self, predicate)
	}
}

impl<I: Iterator> TryFilter for I {}

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

	fn odd_not_three_ref(n: &usize) -> Result<bool, String> {
		if *n == 3 {
			Err(String::from("Error"))
		} else if *n % 2 == 1 {
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

	#[test]
	fn filter_ok() {
		let v = vec![1, 2, 4, 5];

		let result = v
			.into_iter()
			.try_filter(odd_not_three_ref)
			.collect::<Result<Vec<_>, String>>()
			.unwrap();

		assert_eq!(result, vec![1, 5]);
	}

	#[test]
	#[should_panic]
	fn filter_err() {
		let v = vec![2, 4, 6, 8, 3, 10];

		v.into_iter()
			.try_filter(odd_not_three_ref)
			.collect::<Result<Vec<_>, String>>()
			.unwrap();
	}
}
