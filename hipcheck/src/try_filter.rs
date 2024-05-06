// SPDX-License-Identifier: Apache-2.0

//! A struct and trait for performing fallible filtering of Iterators.

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

	fn odd_not_three(n: &usize) -> Result<bool, String> {
		if *n == 3 {
			Err(String::from("Error"))
		} else if *n % 2 == 1 {
			Ok(true)
		} else {
			Ok(false)
		}
	}

	#[test]
	fn filter_ok() {
		let v = vec![1, 2, 4, 5];

		let result = v
			.into_iter()
			.try_filter(odd_not_three)
			.collect::<Result<Vec<_>, String>>()
			.unwrap();

		assert_eq!(result, vec![1, 5]);
	}

	#[test]
	#[should_panic]
	fn filter_err() {
		let v = vec![2, 4, 6, 8, 3, 10];

		v.into_iter()
			.try_filter(odd_not_three)
			.collect::<Result<Vec<_>, String>>()
			.unwrap();
	}
}
