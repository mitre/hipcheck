// SPDX-License-Identifier: Apache-2.0

use crate::{
	org_spec::{Matcher, OrgSpec},
	org_types::Mode,
};
use hipcheck_sdk::prelude::*;

/// A type which encapsulates checking whether a given string matches an org in the orgs file,
/// based on the mode in question. If the mode is Independent, then you're looking for
/// the strings that _don't match_ any of the hosts in the set. If the mode is Affiliated,
/// you're looking for the strings that _match_ one of the hosts in the set.
pub struct Affiliator<'haystack> {
	patterns: Matcher<'haystack>,
	mode: Mode,
}

impl<'haystack> Affiliator<'haystack> {
	/// Check whether the given string is a match for the set of hosts, based on the mode.
	///
	/// If independent mode is on, you're looking for strings which do not match any of
	/// the hosts.
	///
	/// If affiliated mode is on, you're looking for strings which do match one of the
	/// hosts.
	pub fn is_match(&self, s: &str) -> bool {
		match self.mode {
			Mode::Independent => !self.patterns.is_match(s),
			Mode::Affiliated => self.patterns.is_match(s),
			Mode::All => true,
			Mode::None => false,
		}
	}

	/// Construct a new Affiliator from a given OrgSpec (built from an Orgs.kdl file).
	pub fn from_spec(spec: &'haystack OrgSpec) -> Result<Affiliator<'haystack>> {
		let patterns = spec.patterns().map_err(|e| {
			tracing::error!("failed to get patterns for org spec to check against {}", e);
			Error::UnspecifiedQueryState
		})?;
		let mode = spec.mode();
		Ok(Affiliator { patterns, mode })
	}
}
