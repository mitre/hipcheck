// SPDX-License-Identifier: Apache-2.0

//! Organization specification that can be parsed from a KDL file

use crate::{
	org_types::{Mode, OrgList, Strategy},
	util::fs as file,
};
use anyhow::{anyhow, Context as _, Result};
use hipcheck_kdl::extract_data;
use hipcheck_kdl::kdl::KdlDocument;
use miette::Report;
use serde::Deserialize;
use std::{cell::RefCell, collections::HashMap, path::Path, str::FromStr};

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
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct OrgSpec {
	strategy: Strategy,
	orgs: OrgList,
}

impl FromStr for OrgSpec {
	type Err = anyhow::Error;

	fn from_str(s: &str) -> Result<Self> {
		let document =
			// Print miette::Report with Debug for full help text
			KdlDocument::from_str(s).map_err(|e|
				 anyhow!("File doesn't parse as valid KDL:\n{:?}", Report::from(e))
			)?;
		let nodes = document.nodes();

		let strategy: Strategy =
			extract_data(nodes).ok_or_else(|| anyhow!("Could not parse 'strategy'"))?;
		let orgs: OrgList = extract_data(nodes).ok_or_else(|| anyhow!("Could not parse 'orgs'"))?;

		Ok(Self { strategy, orgs })
	}
}

impl OrgSpec {
	/// Load org_spec from the given file.
	pub fn load_from(org_spec_path: &Path) -> Result<OrgSpec> {
		if org_spec_path.is_dir() {
			return Err(anyhow!("Org spec path must be a file, not a directory."));
		}
		file::exists(org_spec_path)?;
		let org_spec_contents = file::read_string(org_spec_path)?;
		let org_spec = OrgSpec::from_str(&org_spec_contents)
			.with_context(|| format!("failed to read org spec file at path {:?}", org_spec_path))?;

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

#[cfg(test)]
mod test {
	use super::OrgSpec;

	use crate::org_types::{
		Host, Mode, Org, OrgList, Strategy, StrategyChild, StrategyCountry, StrategyOrg,
	};
	use pathbuf::pathbuf;
	use std::env;

	#[test]
	fn test_org_spec_parser() {
		let mut strategy = Strategy::new_spec(Mode::Independent);
		let united_states = StrategyCountry::new("United States".to_string());
		let mitre_org = StrategyOrg::new("MITRE".to_string());
		strategy
			.push(StrategyChild::StrategyCountry(united_states))
			.unwrap();
		strategy
			.push(StrategyChild::StrategyOrg(mitre_org))
			.unwrap();

		let mut orgs = OrgList::new();
		let mut hp = Org::new("HP".to_string(), "United States".to_string());
		hp.push(Host::new("hp.com".to_string()));
		hp.push(Host::new("hpe.com".to_string()));
		let mut mitre = Org::new("MITRE".to_string(), "United States".to_string());
		mitre.push(Host::new("mitre.org".to_string()));
		let mut rbc = Org::new("RBC Royal Bank".to_string(), "Canada".to_string());
		rbc.push(Host::new("rbcon.com".to_string()));
		orgs.push(hp);
		orgs.push(mitre);
		orgs.push(rbc);

		let expected = OrgSpec { strategy, orgs };

		let org_spec_path = pathbuf![&env::current_dir().unwrap(), "test", "test_orgs.kdl"];

		let result = OrgSpec::load_from(&org_spec_path).unwrap();

		assert_eq!(expected, result);
	}
}
