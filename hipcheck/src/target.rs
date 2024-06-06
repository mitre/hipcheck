use clap::ValueEnum;
use serde::Serialize;

#[derive(Debug, Clone, ValueEnum, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetType {
	Maven,
	Npm,
	Patch,
	Pypi,
	Repo,
	Request,
	Spdx,
}

impl TargetType {
	pub fn try_resolve_from_target(tgt: &str) -> Option<TargetType> {
		use TargetType::*;
		if let Some(pkg) = tgt.strip_prefix("pkg:") {
			if let Some((pkg_type, _)) = pkg.split_once('/') {
				// Match on purl package type
				match pkg_type {
					"github" => Some(Repo),
					"npm" => Some(Npm),
					"maven" => Some(Maven),
					"pypi" => Some(Pypi),
					_ => None,
				}
			} else {
				None
			}
		} else if tgt.starts_with("https://github.com/") {
			Some(Repo)
		} else if tgt.ends_with(".spdx") {
			Some(Spdx)
		} else {
			None
		}
	}
	pub fn as_str(&self) -> String {
		use serde_json::{to_value, Value};
		let Ok(Value::String(out)) = to_value(self) else {
			unreachable!();
		};
		out
	}
}
