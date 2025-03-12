// SPDX-License-Identifier: Apache-2.0

use itertools::Itertools as _;

/// List a bunch of strings together separated by commas.
pub fn list_with_commas(list: impl IntoIterator<Item = impl ToString>) -> String {
	list.into_iter().map(|elem| elem.to_string()).join(", ")
}
