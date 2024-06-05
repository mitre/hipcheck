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
		if tgt.starts_with("pkg:npm") {
			Some(Npm)
		} else if tgt.ends_with(".spdx") {
			Some(Spdx)
		} else if tgt.ends_with("pkg::github") {
			Some(Repo)
		} else if tgt.starts_with("https://github.com/") {
			Some(Repo)
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
