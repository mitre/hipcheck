// SPDX-License-Identifier: Apache-2.0

/// Calculate the arithmetic mean for a set of floats. Returns an option to account
/// for the possibility of dividing by zero.
pub fn mean(data: &[f64]) -> Option<f64> {
	// Do not use rayon's parallel iter/sum here due to the non-associativity of floating point numbers/math.
	// See: https://en.wikipedia.org/wiki/Associative_property#Nonassociativity_of_floating_point_calculation.
	let sum = data.iter().sum::<f64>();
	let count = data.len();

	match count {
		positive if positive > 0 => Some(sum / count as f64),
		_ => None,
	}
}

/// Calculate the standard deviation for a set of floats. Returns an option to
/// account for the possibility of dividing by zero.
pub fn std_dev(mean: f64, data: &[f64]) -> Option<f64> {
	match (mean, data.len()) {
		(mean, count) if count > 0 => {
			let variance =
				data.iter()
					.map(|value| {
						let diff = mean - *value;
						diff * diff
					})
					.sum::<f64>() / count as f64;

			Some(variance.sqrt())
		}
		_ => None,
	}
}
