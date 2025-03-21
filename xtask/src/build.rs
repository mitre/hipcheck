// SPDX-License-Identifier: Apache-2.0

use crate::{BuildPkg, BuildProfile};
use std::{collections::BTreeSet, fmt::Display};

/// Figure out the `cargo` CLI args for building/checking.
pub fn resolve_builder_args(
	pkgs: &BTreeSet<Pkg>,
	profile: &BuildProfile,
	timings: bool,
) -> impl IntoIterator<Item = String> {
	// Each pkg produces two arguments, plus max two for the profile.
	let mut builder_args = Vec::with_capacity((pkgs.len() * 2) + 2);

	builder_args.extend(profile_args(profile));

	for pkg in pkgs {
		builder_args.extend([String::from("-p"), pkg.to_string()]);
	}

	if timings {
		builder_args.extend([String::from("--timings")]);
	}

	builder_args
}

/// Get the profile flags to use depending on the selection.
pub fn profile_args(profile: &BuildProfile) -> impl IntoIterator<Item = String> {
	match profile {
		BuildProfile::Release => vec![String::from("--release")],
		BuildProfile::Debug => vec![],
		_ => vec![String::from("--profile"), profile.to_string()],
	}
}

/// Resolve a set of packages based on the user's selection.
pub fn resolve_packages(what: BuildPkg) -> Vec<Pkg> {
	let mut vec = vec![];
	extend_with_packages(what, &mut vec);
	vec
}

/// Extend a vec with packages.
fn extend_with_packages(what: BuildPkg, vec: &mut Vec<Pkg>) {
	match what {
		BuildPkg::All => {
			extend_with_packages(BuildPkg::Core, vec);
			extend_with_packages(BuildPkg::Sdk, vec);
			extend_with_packages(BuildPkg::Plugins, vec);
		}
		_ => vec.extend(pkgs_for(what)),
	}
}

/// Get the packages for a given selection.
fn pkgs_for(what: BuildPkg) -> Vec<Pkg> {
	match what {
		BuildPkg::All => {
			unimplemented!("should never be called with 'BuildPkg::All'")
		}
		BuildPkg::Core => vec![Pkg::Hc],
		BuildPkg::Sdk => vec![Pkg::Sdk],
		BuildPkg::Plugins => Pkg::plugins().into_iter().collect(),
	}
}

/// A single package in the workspace, besides 'xtask' itself.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Pkg {
	Hc,
	Sdk,
	Activity,
	Affiliation,
	Binary,
	Churn,
	Entropy,
	Fuzz,
	Git,
	GitHub,
	Identity,
	Linguist,
	Npm,
	Review,
	Typo,
}

impl Display for Pkg {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Pkg::Hc => write!(f, "hipcheck"),
			Pkg::Sdk => write!(f, "hipcheck-sdk"),
			Pkg::Activity => write!(f, "activity"),
			Pkg::Affiliation => write!(f, "affiliation"),
			Pkg::Binary => write!(f, "binary"),
			Pkg::Churn => write!(f, "churn"),
			Pkg::Entropy => write!(f, "entropy"),
			Pkg::Fuzz => write!(f, "fuzz"),
			Pkg::Git => write!(f, "git"),
			Pkg::GitHub => write!(f, "github"),
			Pkg::Identity => write!(f, "identity"),
			Pkg::Linguist => write!(f, "linguist"),
			Pkg::Npm => write!(f, "npm"),
			Pkg::Review => write!(f, "review"),
			Pkg::Typo => write!(f, "typo"),
		}
	}
}

impl Pkg {
	pub fn plugins() -> BTreeSet<Pkg> {
		BTreeSet::from([
			Pkg::Activity,
			Pkg::Affiliation,
			Pkg::Binary,
			Pkg::Churn,
			Pkg::Entropy,
			Pkg::Fuzz,
			Pkg::Git,
			Pkg::GitHub,
			Pkg::Identity,
			Pkg::Linguist,
			Pkg::Npm,
			Pkg::Review,
			Pkg::Typo,
		])
	}
}
