// SPDX-License-Identifier: Apache-2.0

//! Subtypes of an organization specification, with KDL parsing functions

use anyhow::{Context as _, Result, anyhow};
use hipcheck_kdl::kdl::KdlNode;
use hipcheck_kdl::{ParseKdlNode, string_newtype_parse_kdl_node};
use serde::Deserialize;
use std::str::FromStr;
use strum::EnumString;

/// An organization metric strategy. It either implicitly includes _all_
/// organizations in the Orgs struct, or has a more detailed custom specification.
///
/// A strategy with `None` in its `children` field is considered implicit.
/// Otherwise it is considered to have a custom specification.
///
/// Custom specification allows for selection of orgs on both an org-by-org and a
/// country-wide basis. Such specifiers may be combined (for example, analyzing
/// all commits from some /// country, plus commits from an organization not from
/// that country).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Strategy {
	pub mode: Mode,
	pub children: Option<Vec<StrategyChild>>,
}

impl Strategy {
	#[allow(dead_code)]
	pub fn new_spec(mode: Mode) -> Self {
		Self {
			mode,
			children: Some(Vec::new()),
		}
	}

	#[allow(dead_code)]
	pub fn push(&mut self, child: StrategyChild) -> Result<()> {
		match self.children {
			Some(ref mut children) => {
				children.push(child);
				Ok(())
			}
			None => Err(anyhow!("Cannot add specific hosts to an implicit strategy")),
		}
	}

	/// Find all orgs in a given org list matching the org specifiers.
	pub fn orgs_to_analyze<'spec>(&self, full_list: &'spec [Org]) -> Result<Vec<&'spec Org>> {
		if let Some(specifiers) = &self.children {
			let mut orgs = vec![];

			for specifier in specifiers {
				let mut addition = match specifier {
					StrategyChild::StrategyCountry(country) => {
						get_by_country(&country.0, full_list)
							.context("can't resolve country specifier to list of orgs")?
					}
					StrategyChild::StrategyOrg(org) => get_by_name(&org.0, full_list)
						.context("can't resolve name specifier to a specific org")?,
				};

				orgs.append(&mut addition);
			}

			return Ok(orgs);
		}
		Err(anyhow!(
			"Cannot retrieve org specfiers from an implicit strategy"
		))
	}
}

impl ParseKdlNode for Strategy {
	fn kdl_key() -> &'static str {
		"strategy"
	}

	fn parse_node(node: &KdlNode) -> Option<Self> {
		if node.name().to_string().as_str() != Self::kdl_key() {
			return None;
		}

		let mode = Mode::from_str(node.entries().first()?.value().as_string()?).ok()?;

		let children = match node.children() {
			Some(document) => {
				let mut strategy_children = Vec::new();
				for node in document.nodes() {
					if node.name().to_string().as_str() == "country" {
						if let Some(country) = StrategyCountry::parse_node(node) {
							strategy_children.push(StrategyChild::StrategyCountry(country));
						}
					} else if node.name().to_string().as_str() == "org"
						&& let Some(org) = StrategyOrg::parse_node(node)
					{
						strategy_children.push(StrategyChild::StrategyOrg(org));
					}
				}
				Some(strategy_children)
			}
			None => None,
		};

		Some(Self { mode, children })
	}
}

/// The modes for an metric strategy. The analyzer can look for all
/// commits which are independent of the listed orgs, or all commits which are
/// affiliated with the listed orgs. "all" and "none" modes to exclude or include all
/// commits also exist.
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString, Deserialize)]
pub enum Mode {
	#[strum(serialize = "independent")]
	Independent,
	#[strum(serialize = "affiliated")]
	Affiliated,
	#[strum(serialize = "all")]
	All,
	#[strum(serialize = "none")]
	None,
}

/// Identifies whether the specifier is referencing an organization's name or its country.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub enum StrategyChild {
	StrategyCountry(StrategyCountry),
	StrategyOrg(StrategyOrg),
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct StrategyCountry(pub String);
string_newtype_parse_kdl_node!(StrategyCountry, "country");

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct StrategyOrg(pub String);
string_newtype_parse_kdl_node!(StrategyOrg, "org");

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct OrgList(pub Vec<Org>);

impl OrgList {
	pub fn new() -> Self {
		Self(Vec::new())
	}

	pub fn push(&mut self, org: Org) {
		self.0.push(org);
	}
}

impl ParseKdlNode for OrgList {
	fn kdl_key() -> &'static str {
		"orgs"
	}

	fn parse_node(node: &KdlNode) -> Option<Self> {
		if node.name().to_string().as_str() != Self::kdl_key() {
			return None;
		}

		let mut plugins = Self::new();

		for node in node.children()?.nodes() {
			if let Some(dep) = Org::parse_node(node) {
				plugins.push(dep);
			}
		}

		Some(plugins)
	}
}

/// A single organization, with a name, a list of hosts (which form the basis
/// for the hosts used in the analyzer), and an affiliated country.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Org {
	name: String,
	country: String,
	children: Vec<Host>,
}

impl Org {
	#[allow(dead_code)]
	pub fn new(name: String, country: String) -> Self {
		Self {
			name,
			country,
			children: Vec::new(),
		}
	}

	#[allow(dead_code)]
	pub fn push(&mut self, child: Host) {
		self.children.push(child);
	}

	/// Return the hosts in the org as `&str`
	pub fn hosts(&self) -> Vec<&str> {
		let mut hosts = Vec::new();
		for host in self.children.iter() {
			hosts.push(host.0.as_str());
		}

		hosts
	}
}

impl ParseKdlNode for Org {
	fn kdl_key() -> &'static str {
		"org"
	}

	fn parse_node(node: &KdlNode) -> Option<Self> {
		if node.name().to_string().as_str() != Self::kdl_key() {
			return None;
		}

		let name = node.entries().first()?.value().as_string()?.to_string();
		let country = node.get("country")?.as_string()?.to_string();

		let mut children = Vec::new();
		for node in node.children()?.nodes() {
			children.push(Host::parse_node(node)?)
		}

		Some(Self {
			name,
			country,
			children,
		})
	}
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Host(pub String);
string_newtype_parse_kdl_node!(Host, "host");

/// Filter a list of orgs based on the country they're affiliated with.
fn get_by_country<'spec>(country: &str, list: &'spec [Org]) -> Result<Vec<&'spec Org>> {
	let orgs: Vec<_> = list.iter().filter(|org| org.country == country).collect();

	if orgs.is_empty() {
		Err(anyhow!("invalid country name '{}'", country))
	} else {
		Ok(orgs)
	}
}

/// Find a specific org in a list of orgs.
///
/// Returns a Vec<Org> with one element, for symmetry with `get_by_country`.
fn get_by_name<'spec>(name: &str, list: &'spec [Org]) -> Result<Vec<&'spec Org>> {
	let org = list.iter().find(|org| org.name == name);

	match org {
		Some(org) => Ok(vec![org]),
		None => Err(anyhow!("invalid org name '{}'", name)),
	}
}
