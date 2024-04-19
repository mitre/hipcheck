// SPDX-License-Identifier: Apache-2.0

use crate::MetricProvider;
use hc_common::{
	log,
	serde::{
		self,
		de::{Error as SerdeError, Visitor},
		Deserialize, Deserializer, Serialize,
	},
};
use hc_data::git::{Commit, CommitContributorView};
use hc_error::{hc_error, Context as _, Error, Result};
use hc_filesystem as file;
use std::cell::RefCell;
use std::collections::HashMap;
use std::convert::{TryFrom, TryInto};
use std::fmt;
use std::ops::Not as _;
use std::path::Path;
use std::rc::Rc;
use std::result::Result as StdResult;

#[derive(Debug, Eq, PartialEq, Serialize)]
#[serde(crate = "self::serde")]
pub struct AffiliationOutput {
	pub affiliations: Vec<Affiliation>,
}

#[derive(Debug, Eq, PartialEq, Serialize)]
#[serde(crate = "self::serde")]
pub struct Affiliation {
	pub commit: Rc<Commit>,
	pub affiliated_type: AffiliatedType,
}

#[derive(Debug, Eq, PartialEq, Serialize, Clone, Copy)]
#[serde(crate = "self::serde")]
pub enum AffiliatedType {
	Author,
	Committer,
	Both,
	Neither,
}

impl AffiliatedType {
	fn is(affiliator: &Affiliator, commit_view: &CommitContributorView) -> AffiliatedType {
		let author_is_match = affiliator.is_match(&commit_view.author.email);
		let committer_is_match = affiliator.is_match(&commit_view.committer.email);

		match (author_is_match, committer_is_match) {
			(true, true) => AffiliatedType::Both,
			(true, false) => AffiliatedType::Author,
			(false, true) => AffiliatedType::Committer,
			(false, false) => AffiliatedType::Neither,
		}
	}

	pub fn is_affiliated(&self) -> bool {
		matches!(self, AffiliatedType::Neither).not()
	}
}

pub(crate) fn affiliation_metric(db: &dyn MetricProvider) -> Result<Rc<AffiliationOutput>> {
	log::debug!("running affiliation metric");

	// Parse the Orgs file and construct an OrgSpec.
	let org_spec = OrgSpec::from_file(&db.orgs_file()).context("failed to load org spec")?;

	// Get the commits for the source.
	let commits = db
		.commits()
		.context("failed to get commits for affiliation metric")?;

	// Use the OrgSpec to build an Affiliator.
	let affiliator = Affiliator::from_spec(&org_spec)
		.context("failed to build affiliation checker from org spec")?;

	// Construct a big enough Vec for the affiliation info.
	let mut affiliations = Vec::with_capacity(commits.len());

	for commit in commits.iter() {
		// Check if a commit matches the affiliation rules.
		let commit_view = db
			.contributors_for_commit(Rc::clone(commit))
			.context("failed to get commits")?;

		let affiliated_type = AffiliatedType::is(&affiliator, &commit_view);

		affiliations.push(Affiliation {
			commit: Rc::clone(commit),
			affiliated_type,
		});
	}

	log::info!("completed affiliation metric");

	Ok(Rc::new(AffiliationOutput { affiliations }))
}

pub(crate) fn pr_affiliation_metric(db: &dyn MetricProvider) -> Result<Rc<AffiliationOutput>> {
	log::debug!("running pull request affiliation metric");

	// Parse the Orgs file and construct an OrgSpec.
	let org_spec = OrgSpec::from_file(&db.orgs_file()).context("failed to load org spec")?;

	// Get the commits for the source.

	let pull_request = db
		.single_pull_request_review()
		.context("failed to get pull request")?;

	let commits = &pull_request.as_ref().commits;

	// Use the OrgSpec to build an Affiliator.
	let affiliator = Affiliator::from_spec(&org_spec)
		.context("failed to build affiliation checker from org spec")?;

	// Construct a big enough Vec for the affiliation info.
	let mut affiliations = Vec::with_capacity(commits.len());

	for commit in commits.iter() {
		// Check if a commit matches the affiliation rules.
		let commit_view = db
			.get_pr_contributors_for_commit(Rc::clone(commit))
			.context("failed to get commits")?;

		let affiliated_type = AffiliatedType::is(&affiliator, &commit_view);

		affiliations.push(Affiliation {
			commit: Rc::clone(commit),
			affiliated_type,
		});
	}

	log::info!("completed pull request affiliation metric");

	Ok(Rc::new(AffiliationOutput { affiliations }))
}

/// A type which encapsulates checking whether a given string matches an org in theorgs file,
/// based on the mode in question. If the mode is Independent, then you're looking for
/// the strings that don't match any of the hosts in the set. If the mode is Affiliated,
/// you're looking for the strings that match one of the hosts in the set.
struct Affiliator<'haystack> {
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
	fn is_match(&self, s: &str) -> bool {
		match self.mode {
			Mode::Independent => !self.patterns.is_match(s),
			Mode::Affiliated => self.patterns.is_match(s),
			Mode::All => true,
			Mode::None => false,
		}
	}

	/// Construct a new Affiliator from a given OrgSpec (built from an Orgs.toml file).
	fn from_spec(spec: &'haystack OrgSpec) -> Result<Affiliator<'haystack>> {
		let patterns = spec.patterns()?;
		let mode = spec.mode();
		Ok(Affiliator { patterns, mode })
	}
}

#[derive(Default)]
struct Matcher<'haystack> {
	cache: RefCell<HashMap<String, bool>>,
	hosts: Vec<&'haystack str>,
}

impl<'haystack> Matcher<'haystack> {
	fn new(hosts: Vec<&'haystack str>) -> Matcher<'haystack> {
		Matcher {
			hosts,
			..Matcher::default()
		}
	}

	fn is_match(&self, s: &str) -> bool {
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
#[derive(Deserialize)]
#[serde(crate = "self::serde")]
struct OrgSpec {
	strategy: Strategy,
	orgs: Vec<Org>,
}

impl OrgSpec {
	fn from_file(orgs_file: &Path) -> Result<OrgSpec> {
		let raw: RawOrgSpec = file::read_toml(orgs_file).context("failed to read orgs file")?;
		raw.try_into()
	}

	/// Get the patterns to check against based on the org spec contents.
	fn patterns(&self) -> Result<Matcher<'_>> {
		match self.strategy {
			Strategy::All(_) => {
				let mut hosts = Vec::new();

				for org in &self.orgs {
					for host in &org.hosts {
						hosts.push(host.as_ref());
					}
				}

				Ok(Matcher::new(hosts))
			}
			Strategy::Specified(ref spec) => {
				let mut hosts = Vec::new();

				for org in &spec
					.orgs_to_analyze(&self.orgs)
					.context("can't resolve orgs to analyze from spec")?
				{
					for host in &org.hosts {
						hosts.push(host.as_ref());
					}
				}

				Ok(Matcher::new(hosts))
			}
		}
	}

	/// Get the mode associated with the OrgSpec.
	fn mode(&self) -> Mode {
		match self.strategy {
			Strategy::All(mode) => mode,
			Strategy::Specified(ref strategy) => strategy.mode,
		}
	}
}

/// An organization metric strategy. It either implicitly includes _all_
/// organizations in the Orgs struct, or has a more detailed custom specification.
#[derive(Deserialize)]
#[serde(crate = "self::serde")]
enum Strategy {
	All(Mode),
	Specified(Spec),
}

/// The two modes for an metric strategy. The analyzer can either look for all
/// commits which are independent of the listed orgs, or all commits which are
/// affiliated with the listed orgs.
#[derive(Deserialize, Clone, Copy)]
#[serde(crate = "self::serde")]
enum Mode {
	Independent,
	Affiliated,
	All,
	None,
}

/// A specification for a custom strategy. Includes a mode (independent or
/// affiliated), and a list of organization specifiers. This allows for
/// selection of orgs on both an org-by-org and a country-wide basis. Such
/// specifiers may be combined (for example, analyzing all commits from some
/// country, plus commits from an organization not from that country).
#[derive(Deserialize)]
#[serde(crate = "self::serde")]
struct Spec {
	mode: Mode,
	list: Vec<Reference>,
}

impl Spec {
	/// Find all orgs in a given org list matching the org specifiers.
	fn orgs_to_analyze<'spec>(&self, full_list: &'spec [Org]) -> Result<Vec<&'spec Org>> {
		let mut orgs = vec![];

		for specifier in &self.list {
			let mut addition = match specifier.kind {
				Kind::Country => get_by_country(&specifier.value, full_list)
					.context("can't resolve country specifier to list of orgs")?,
				Kind::Name => get_by_name(&specifier.value, full_list)
					.context("can't resolve name specifier to a specific org")?,
			};

			orgs.append(&mut addition);
		}

		Ok(orgs)
	}
}

/// Filter a list of orgs based on the country they're affiliated with.
fn get_by_country<'spec>(country: &str, list: &'spec [Org]) -> Result<Vec<&'spec Org>> {
	let orgs: Vec<_> = list.iter().filter(|org| org.country == country).collect();

	if orgs.is_empty() {
		Err(hc_error!("invalid country name '{}'", country))
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
		None => Err(hc_error!("invalid org name '{}'", name)),
	}
}

/// A single organization, with a name, a list of hosts (which form the basis
/// for the hosts used in the analyzer), and an affiliated country.
#[derive(Clone, Deserialize, Debug)]
#[serde(crate = "self::serde")]
struct Org {
	name: String,
	hosts: Vec<String>,
	country: String,
}

/// An org specifier, which marks whether an organization's country or name is
/// being referenced, and carries the actual value of the reference.
#[derive(Debug)]
struct Reference {
	kind: Kind,
	value: String,
}

/// A specifier kind, which identifies whether the specifier is referencing an
/// organization's name or its country.
#[derive(Debug)]
enum Kind {
	Country,
	Name,
}

impl<'a> TryFrom<&'a str> for Kind {
	type Error = Error;

	fn try_from(s: &'a str) -> StdResult<Self, Self::Error> {
		match s {
			"country" => Ok(Kind::Country),
			"org" => Ok(Kind::Name),
			_ => Err(hc_error!(
				"invalid org reference '{}' (expected: 'country' or 'org')",
				s
			)),
		}
	}
}

impl<'de> Deserialize<'de> for Reference {
	fn deserialize<D>(deserializer: D) -> StdResult<Self, D::Error>
	where
		D: Deserializer<'de>,
	{
		deserializer.deserialize_str(ReferenceVisitor)
	}
}

struct ReferenceVisitor;

impl<'de> Visitor<'de> for ReferenceVisitor {
	type Value = Reference;

	fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
		formatter.write_str("a string representing an org spec reference")
	}

	// Split string on colon.
	// Left side is the kind (should be "Country" or "Name")
	// Right side is the value
	fn visit_str<E>(self, s: &str) -> StdResult<Self::Value, E>
	where
		E: SerdeError,
	{
		let parts: Vec<&str> = s.split(':').collect();

		if parts.len() != 2 {
			return Err(SerdeError::custom("invalid reference"));
		}

		let kind = parts[0].try_into().map_err(SerdeError::custom)?;
		let value = parts[1].into();
		Ok(Reference { kind, value })
	}
}

#[derive(Deserialize, Debug)]
#[serde(crate = "self::serde")]
struct RawOrgSpec {
	strategy: Option<String>,
	strategy_spec: Option<RawSpec>,
	orgs: Vec<Org>,
}

#[derive(Deserialize, Debug)]
#[serde(crate = "self::serde")]
struct RawSpec {
	mode: String,
	list: Vec<Reference>,
}

impl TryInto<OrgSpec> for RawOrgSpec {
	type Error = Error;

	// PANIC: Safe to unwrap the try_into() functions, because if there is nothing to convert, the match will go to a different arm.
	fn try_into(self) -> StdResult<OrgSpec, Self::Error> {
		let strategy = match (self.strategy, self.strategy_spec) {
			(Some(strat), None) => {
				// Convert the strat String into a Strategy::All(Mode) with a TryFrom for String -> Mode
				Strategy::All(strat.try_into().map_err(|_| {
					hc_error!("failed to convert strat String into a Strategy::All(Mode)")
				})?)
			}
			(None, Some(spec)) => {
				// Convert the RawSpec into a Spec (add its own TryInto impl)
				Strategy::Specified(
					spec.try_into()
						.map_err(|_| hc_error!("failed to convert RawSpec into a Spec"))?,
				)
			}
			(None, None) => {
				// Default: Use the Strategy::All(Mode::Affiliated)
				Strategy::All(Mode::Affiliated)
			}
			(Some(_), Some(_)) => {
				// Error: Can't have both a strategy and a strategy_spec
				return Err(Error::msg("ambiguous strategy specifier (orgs file can't contain both 'strategy' and 'strategy_spec')"));
			}
		};

		let orgs = self.orgs;

		Ok(OrgSpec { strategy, orgs })
	}
}

impl TryFrom<String> for Mode {
	type Error = Error;

	fn try_from(s: String) -> StdResult<Self, Self::Error> {
		match s.as_ref() {
			"affiliated" => Ok(Mode::Affiliated),
			"independent" => Ok(Mode::Independent),
			"all" => Ok(Mode::All),
			"none" => Ok(Mode::None),
			_ => Err(hc_error!(
				"invalid mode '{}' (expected: 'affiliated', 'independent', 'all' or 'none')",
				s
			)),
		}
	}
}

impl TryInto<Spec> for RawSpec {
	type Error = Error;

	fn try_into(self) -> StdResult<Spec, Self::Error> {
		let mode: Mode = self.mode.try_into()?;
		let list = self.list;

		Ok(Spec { mode, list })
	}
}
