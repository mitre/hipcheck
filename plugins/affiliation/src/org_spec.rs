// SPDX-License-Identifier: Apache-2.0

use crate::{org_types::{Mode, OrgList, Strategy}, util::{fs as file, kdl::extract_data}};
use anyhow::{anyhow, Context as _, Result};
use kdl::KdlDocument;
use std::{
    cell::RefCell,
	collections::HashMap,
    str::FromStr,
	path::Path,
};

#[derive(Default)]
pub struct Matcher<'haystack> {
	cache: RefCell<HashMap<String, bool>>,
	hosts: Vec<&'haystack str>,
}

impl<'haystack> Matcher<'haystack> {
	pub fn new(hosts: Vec<&'haystack str>) -> Matcher<'haystack> {
		Matcher {
			hosts,
			..Matcher::default()
		}
	}

	pub fn is_match(&self, s: &str) -> bool {
		if let Some(prior_result) = self.cache.borrow().get(s) {
			return *prior_result;
		}

		for host in &self.hosts {
			if s.ends_with(host) {
				self.cache.borrow_mut().insert(s.to_owned(), true);
				return true;
			}
		}

		false
	}
}

/// An overall organization metric specification, with a strategy for how the
/// metric will be performed, and a list of organizations.
pub struct OrgSpec {
	strategy: Strategy,
	orgs: OrgList,
}

impl FromStr for OrgSpec {
	type Err = anyhow::Error;

	fn from_str(s: &str) -> Result<Self> {
		let document =
			KdlDocument::from_str(s).map_err(|e| anyhow!("Error parsing org spec file: {}", e))?;
		let nodes = document.nodes();

		let strategy: Strategy = extract_data(nodes).ok_or_else(|| anyhow!("Could not parse 'strategy'"))?;
        let orgs: OrgList = extract_data(nodes).ok_or_else(|| anyhow!("Could not parse 'orgs'"))?;
		
		Ok(Self {
			strategy,
            orgs
		})
	}
}

impl OrgSpec {
	/// Load org_spec from the given file.
	pub fn load_from(org_spec_path: &Path) -> Result<OrgSpec> {
		if org_spec_path.is_dir() {
			return Err(anyhow!(
				"Org spec path must be a file, not a directory."
			));
		}
		file::exists(org_spec_path)?;
		let org_spec = OrgSpec::from_str(&file::read_string(org_spec_path)?)?;

		Ok(org_spec)
	}

	/// Get the patterns to check against based on the org spec contents.
	pub fn patterns(&self) -> Result<Matcher<'_>> {
		if self.strategy.children.is_none() {
            let mut hosts = Vec::new();

            for org in &self.orgs.0 {
                for host in org.hosts() {
                    hosts.push(host);
                }
            }

            Ok(Matcher::new(hosts))
        } else { 
            let mut hosts = Vec::new();

            for org in &self
                .strategy
                .orgs_to_analyze(&self.orgs.0)
                .context("can't resolve orgs to analyze from spec")?
            {
                for host in org.hosts() {
                    hosts.push(host);
                }
            }

            Ok(Matcher::new(hosts))
		}
	}

	/// Get the mode associated with the OrgSpec.
	pub fn mode(&self) -> Mode {
		self.strategy.mode
	}
}