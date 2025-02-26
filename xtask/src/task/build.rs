// SPDX-License-Identifier: Apache-2.0

use crate::{BuildArgs, BuildPkg, BuildProfile};
use anyhow::Result;
use itertools::Itertools;
use log::debug;
use std::{collections::BTreeSet, fmt::Display};
use xshell::{cmd, Shell};

/// Run the build command.
pub fn run(args: BuildArgs) -> Result<()> {
	debug!("rebuild targeting: {}", list_with_commas(&args.pkg));

	let pkgs = args
		.pkg
		.into_iter()
		.flat_map(resolve_packages)
		.unique()
		.collect::<BTreeSet<_>>();

	// Each pkg produces two arguments, plus max two for the profile.
	let mut builder_args = Vec::with_capacity((pkgs.len() * 2) + 2);

	builder_args.extend(profile_args(&args.profile));

	for pkg in &pkgs {
		builder_args.extend([String::from("-p"), pkg.to_string()]);
	}

	debug!("rebuilding packages: {}", list_with_commas(&pkgs));
	debug!("using profile: {}", args.profile.to_string());

	let sh = Shell::new()?;
	cmd!(sh, "cargo build {builder_args...}").run()?;

	Ok(())
}

/// List a bunch of strings together separated by commas.
fn list_with_commas(pkgs: impl IntoIterator<Item = impl ToString>) -> String {
	pkgs.into_iter().map(|elem| elem.to_string()).join(", ")
}

/// Get the profile flags to use depending on the selection.
fn profile_args(profile: &BuildProfile) -> impl IntoIterator<Item = String> {
	match profile {
		BuildProfile::Release => vec![String::from("--release")],
		BuildProfile::Debug => vec![],
		_ => vec![String::from("--profile"), profile.to_string()],
	}
}

/// Resolve a set of packages based on the user's selection.
fn resolve_packages(what: BuildPkg) -> Vec<Pkg> {
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
enum Pkg {
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
	fn plugins() -> BTreeSet<Pkg> {
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
