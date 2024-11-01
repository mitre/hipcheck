// SPDX-License-Identifier: Apache-2.0

mod error;
mod linguist;
mod metric;
mod types;

use crate::{linguist::*, metric::*, types::*};

use clap::Parser;
use hipcheck_sdk::{prelude::*, types::Target};
use serde::Deserialize;
use tokio::sync::Mutex;

use std::{
	path::PathBuf,
	result::Result as StdResult,
	sync::{Arc, OnceLock},
};

#[derive(Deserialize)]
struct RawConfig {
	#[serde(rename = "langs-file")]
	langs_file: Option<PathBuf>,
	#[serde(rename = "entropy-threshold")]
	entropy_threshold: Option<f64>,
	#[serde(rename = "commit-percentage")]
	commit_percentage: Option<f64>,
}

#[derive(Clone, Debug)]
struct PolicyExprConf {
	pub entropy_threshold: f64,
	pub commit_percentage: f64,
}

struct Config {
	langs_file: PathBuf,
	opt_policy: Option<PolicyExprConf>,
}

impl TryFrom<RawConfig> for Config {
	type Error = hipcheck_sdk::error::ConfigError;
	fn try_from(value: RawConfig) -> StdResult<Config, Self::Error> {
		// Langs file field must always be present
		let Some(langs_file) = value.langs_file else {
			return Err(ConfigError::MissingRequiredConfig {
				field_name: "langs-file".to_owned(),
				field_type: "string".to_owned(),
				possible_values: vec![],
			});
		};
		// Default policy expr depends on two fields. If neither present, no default
		// policy. else make sure both are present
		let opt_policy = match (value.entropy_threshold, value.commit_percentage) {
			(None, None) => None,
			(Some(_), None) => {
				return Err(ConfigError::MissingRequiredConfig {
					field_name: "commit-percentage".to_owned(),
					field_type: "float".to_owned(),
					possible_values: vec![],
				});
			}
			(None, Some(_)) => {
				return Err(ConfigError::MissingRequiredConfig {
					field_name: "entropy-threshold".to_owned(),
					field_type: "float".to_owned(),
					possible_values: vec![],
				});
			}
			(Some(entropy_threshold), Some(commit_percentage)) => Some(PolicyExprConf {
				entropy_threshold,
				commit_percentage,
			}),
		};
		// Sanity check on policy expr config
		if let Some(policy_ref) = &opt_policy {
			if policy_ref.commit_percentage < 0.0 || policy_ref.commit_percentage > 1.0 {
				return Err(ConfigError::InvalidConfigValue {
					field_name: "commit-percentage".to_owned(),
					value: policy_ref.commit_percentage.to_string(),
					reason: "percentage must be between 0.0 and 1.0, inclusive".to_owned(),
				});
			}
		}
		Ok(Config {
			langs_file,
			opt_policy,
		})
	}
}

pub static DATABASE: OnceLock<Arc<Mutex<Linguist>>> = OnceLock::new();

#[query]
async fn commit_entropies(
	_engine: &mut PluginEngine,
	commit_diffs: Vec<CommitDiff>,
) -> Result<Vec<CommitEntropy>> {
	// Calculate the grapheme frequencies for each commit which contains code.
	let mut filtered: Vec<CommitDiff> = vec![];
	let linguist = DATABASE
		.get()
		.ok_or(Error::UnspecifiedQueryState)?
		.lock()
		.await;
	for cd in commit_diffs.into_iter() {
		if is_likely_source_file_cd(&linguist, &cd) {
			filtered.push(cd);
		}
	}
	let commit_freqs = filtered
		.iter()
		.map(|x| grapheme_freqs(&linguist, x))
		.collect::<Vec<CommitGraphemeFreq>>();

	drop(linguist);

	// Calculate baseline grapheme frequencies across all commits which contain code.
	let baseline_freqs = baseline_freqs(&commit_freqs);

	// Calculate the entropy of each commit which contains code.
	let mut commit_entropies = commit_freqs
		.iter()
		.map(|commit_freq| commit_entropy(commit_freq, &baseline_freqs))
		.collect::<Vec<_>>();

	// Sort the commits	by entropy score
	// PANIC: It is safe to unwrap here, because the entropy scores will always be valid floating point numbers if we get to this point
	commit_entropies.sort_by(|a, b| b.entropy.partial_cmp(&a.entropy).unwrap());

	// Convert to Z-scores and return results.
	z_scores(commit_entropies).map_err(|_| Error::UnspecifiedQueryState)
}

#[query(default)]
async fn entropy(engine: &mut PluginEngine, value: Target) -> Result<Vec<f64>> {
	let local = value.local;
	let val_commits = engine.query("mitre/git/commit_diffs", local).await?;
	let commits: Vec<CommitDiff> =
		serde_json::from_value(val_commits).map_err(Error::InvalidJsonInQueryOutput)?;
	Ok(commit_entropies(engine, commits)
		.await?
		.iter()
		.map(|o| o.entropy)
		.collect())
}

#[derive(Clone, Debug, Default)]
struct EntropyPlugin {
	policy_conf: OnceLock<Option<PolicyExprConf>>,
}

impl Plugin for EntropyPlugin {
	const PUBLISHER: &'static str = "mitre";
	const NAME: &'static str = "entropy";
	fn set_config(&self, config: Value) -> StdResult<(), ConfigError> {
		// Deserialize and validate the config struct
		let conf: Config = serde_json::from_value::<RawConfig>(config)
			.map_err(|e| ConfigError::Unspecified {
				message: e.to_string(),
			})?
			.try_into()?;
		// Store the PolicyExprConf to be accessed only in the `default_policy_expr()` impl
		self.policy_conf
			.set(conf.opt_policy)
			.map_err(|_| ConfigError::Unspecified {
				message: "plugin was already configured".to_string(),
			})?;
		let sfd =
			SourceFileDetector::load(conf.langs_file).map_err(|e| ConfigError::Unspecified {
				message: e.to_string(),
			})?;
		let mut database = Linguist::new();
		database.set_source_file_detector(Arc::new(sfd));
		let global_db = Arc::new(Mutex::new(database));
		DATABASE
			.set(global_db)
			.map_err(|_e| ConfigError::Unspecified {
				message: "config was already set".to_owned(),
			})
	}

	fn default_policy_expr(&self) -> Result<String> {
		match self.policy_conf.get() {
			None => Err(Error::UnspecifiedQueryState),
			// If no policy vars, we have no default expr
			Some(None) => Ok("".to_owned()),
			// Use policy config vars to construct a default expr
			Some(Some(policy_conf)) => Ok(format!(
				"(lte (divz (count (filter (gt {}) $)) (count $)) {})",
				policy_conf.entropy_threshold, policy_conf.commit_percentage
			)),
		}
	}

	fn explain_default_query(&self) -> Result<Option<String>> {
		Ok(Some(
			"The entropy calculation of each commit in a repo".to_owned(),
		))
	}

	queries! {}
}

#[derive(Parser, Debug)]
struct Args {
	#[arg(long)]
	port: u16,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
	let args = Args::try_parse().unwrap();
	PluginServer::register(EntropyPlugin::default())
		.listen(args.port)
		.await
}
