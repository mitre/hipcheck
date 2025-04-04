// SPDX-License-Identifier: Apache-2.0
use std::{fmt::Display, fs::File, io::Read, path::Path, str::FromStr};

use anyhow::anyhow;
use kdl::KdlDocument;
use serde::{Deserialize, Serialize};

use crate::task::manifest::ParseKdlNode;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BenchmarkTargetType {
	/// plain git repository
	Repo,
	/// Python package repository
	PyPI,
	/// Java package repository
	Maven,
	/// NodeJS package repository
	#[allow(clippy::upper_case_acronyms)]
	NPM,
}

impl Display for BenchmarkTargetType {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			BenchmarkTargetType::Repo => write!(f, "repo"),
			BenchmarkTargetType::PyPI => write!(f, "pypi"),
			BenchmarkTargetType::Maven => write!(f, "maven"),
			BenchmarkTargetType::NPM => write!(f, "npm"),
		}
	}
}

impl FromStr for BenchmarkTargetType {
	type Err = anyhow::Error;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s.to_lowercase().trim() {
			"repo" => Ok(Self::Repo),
			"pypi" => Ok(Self::PyPI),
			"maven" => Ok(Self::Maven),
			"npm" => Ok(Self::NPM),
			_ => Err(anyhow!("{} is not a valid BenchmarkTargetType", s)),
		}
	}
}

pub struct BenchmarkTargets(pub Vec<BenchmarkTarget>);

impl BenchmarkTargets {
	pub fn from_file(path: impl AsRef<Path>) -> anyhow::Result<Self> {
		let mut contents = String::new();
		File::open(path)?.read_to_string(&mut contents)?;
		let targets = Self::from_str(&contents)?;
		Ok(targets)
	}
}

impl FromStr for BenchmarkTargets {
	type Err = anyhow::Error;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		let document =
			KdlDocument::from_str(s).map_err(|e| anyhow!("Error parsing as KDL: {}", e))?;

		let mut targets = vec![];
		for node in document
			.nodes()
			.first()
			.ok_or(anyhow!("missing 'targets' keyword"))?
			.children()
			.ok_or(anyhow!("no children found for 'targets'"))?
			.nodes()
		{
			let target = BenchmarkTarget::parse_node(node)
				.ok_or(anyhow!("Error parsing node as BenchmarkTarget"))?;
			targets.push(target);
		}
		Ok(Self(targets))
	}
}

#[derive(Clone, Debug)]
pub struct BenchmarkTarget {
	target_type: BenchmarkTargetType,
	url: String,
	reference: String,
}

impl BenchmarkTarget {
	pub fn target_type(&self) -> BenchmarkTargetType {
		self.target_type
	}

	pub fn url(&self) -> &str {
		&self.url
	}

	pub fn reference(&self) -> &str {
		&self.reference
	}
}

impl ParseKdlNode for BenchmarkTarget {
	fn kdl_key() -> &'static str {
		"repo"
	}

	fn parse_node(node: &kdl::KdlNode) -> Option<Self> {
		if node.name().to_string().as_str() != Self::kdl_key() {
			return None;
		}
		let url = node.entries().first()?.value().as_string()?.to_string();
		let reference = node.entries().get(1)?.value().as_string()?.to_string();
		let target_type =
			BenchmarkTargetType::from_str(node.entries().get(2)?.value().as_string()?).ok()?;
		let benchmark_target = Self {
			target_type,
			url,
			reference,
		};
		Some(benchmark_target)
	}
}

#[cfg(all(test, target_os = "linux"))]
mod test {
	use std::str::FromStr;

	use super::BenchmarkTargets;

	#[test]
	fn ensure_target_kdl_is_valid() {
		let contents = include_str!("../../../../../config/benchmark-targets.kdl");
		BenchmarkTargets::from_str(contents).unwrap();
	}
}
