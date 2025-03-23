use std::{fmt::Display, num::NonZeroU8};

use crate::task::manifest::ParseKdlNode;

#[derive(Clone, Debug)]
pub enum BenchmarkTargetType {
	/// plain git repository
	Repo,
	/// Python package repository
	PyPI,
	/// Java package repository
	Maven,
	// TODO: add npm to this
}

impl BenchmarkTargetType {
	/// convert BenchmarkTargetType to what would be passed as `-t` when using `hc check`
	pub fn as_str(&self) -> &'static str {
		match self {
			BenchmarkTargetType::Repo => "repo",
			BenchmarkTargetType::PyPI => "pypi",
			BenchmarkTargetType::Maven => "maven",
		}
	}
}

#[derive(Clone, Debug)]
pub struct BenchmarkTarget {
	name: String,
	url: String,
	ref_type: String,
	target: BenchmarkTargetType,
}

impl ParseKdlNode for BenchmarkTarget {
	fn kdl_key() -> &'static str {
		todo!()
	}

	fn parse_node(node: &kdl::KdlNode) -> Option<Self> {
		todo!()
	}
}

pub struct BenchmarkConfig {
	/// all of the targets to benchmark against
	target: Vec<BenchmarkTarget>,
	/// number of runs to perform for each target
	runs: NonZeroU8,
}
