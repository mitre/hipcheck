// SPDX-License-Identifier: Apache-2.0

mod error;
mod metric;
mod types;

use crate::{metric::*, types::*};

use clap::Parser;
use hipcheck_sdk::{prelude::*, types::Target, LogLevel};
use serde::Deserialize;

use std::{collections::HashSet, path::PathBuf, result::Result as StdResult, sync::OnceLock};

#[derive(Deserialize)]
struct RawConfig {
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
	opt_policy: Option<PolicyExprConf>,
}

impl TryFrom<RawConfig> for Config {
	type Error = hipcheck_sdk::error::ConfigError;
	fn try_from(value: RawConfig) -> StdResult<Config, Self::Error> {
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
		Ok(Config { opt_policy })
	}
}

#[query]
async fn commit_entropies(
	engine: &mut PluginEngine,
	mut commit_diffs: Vec<CommitDiff>,
) -> Result<Vec<CommitEntropy>> {
	tracing::info!("running commit_entropies query");
	let mut possible_source_files = HashSet::<PathBuf>::new();
	for cd in commit_diffs.iter() {
		possible_source_files.extend(
			cd.diff
				.file_diffs
				.iter()
				.map(|y| PathBuf::from(&y.file_name)),
		);
	}
	let psf_vec = possible_source_files.into_iter().collect::<Vec<_>>();

	let psf_val_vec = psf_vec
		.iter()
		.map(serde_json::to_value)
		.collect::<StdResult<Vec<Value>, serde_json::Error>>()
		.map_err(|_| Error::UnspecifiedQueryState)?;

	let res = engine.batch_query("mitre/linguist", psf_val_vec).await?;
	let psf_bools: Vec<bool> = serde_json::from_value(serde_json::Value::Array(res))
		.map_err(|_| Error::UnspecifiedQueryState)?;

	if psf_bools.len() != psf_vec.len() {
		return Err(Error::UnspecifiedQueryState);
	}

	let source_files: HashSet<String> = psf_vec
		.into_iter()
		.zip(psf_bools.into_iter())
		.filter_map(|(p, b)| if b { Some(p) } else { None })
		.map(|p| p.as_path().to_string_lossy().into_owned())
		.collect();

	// Calculate the grapheme frequencies for each commit which contains code.
	commit_diffs.retain(|cd| {
		cd.diff
			.file_diffs
			.iter()
			.any(|x| source_files.contains(&x.file_name))
	});
	let commit_freqs = commit_diffs
		.iter()
		.map(|x| grapheme_freqs(&source_files, x))
		.collect::<Vec<CommitGraphemeFreq>>();

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

	tracing::info!("completed commit_entropies query");
	// Convert to Z-scores and return results.
	z_scores(commit_entropies).map_err(|_| Error::UnspecifiedQueryState)
}

#[query(default)]
async fn entropy(engine: &mut PluginEngine, value: Target) -> Result<Vec<f64>> {
	tracing::info!("running entropy query");
	let policy_config = POLICY_CONFIG.get().cloned().flatten();
	let local = value.local;
	let val_commits = engine.query("mitre/git/commit_diffs", local).await?;
	let commits: Vec<CommitDiff> =
		serde_json::from_value(val_commits).map_err(Error::InvalidJsonInQueryOutput)?;
	let commit_entropies: Vec<f64> = commit_entropies(engine, commits)
		.await?
		.iter()
		.map(|o| {
			if policy_config
				.as_ref()
				.is_some_and(|policy_config| policy_config.entropy_threshold < o.entropy)
			{
				engine.record_concern(format!(
					"Commit hash: {}, Entropy: {}",
					o.commit.hash, o.entropy
				));
			}
			o.entropy
		})
		.collect();
	tracing::info!("completed entropy query");
	Ok(commit_entropies)
}

#[derive(Clone, Debug, Default)]
struct EntropyPlugin {}
// Define a global OnceLock variable to store the policy_config so it can be accessed by entropy
static POLICY_CONFIG: OnceLock<Option<PolicyExprConf>> = OnceLock::new();

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
		POLICY_CONFIG
			.set(conf.opt_policy)
			.map_err(|_| ConfigError::InternalError {
				message: "plugin was already configured".to_string(),
			})
	}
	fn default_policy_expr(&self) -> Result<String> {
		match POLICY_CONFIG.get() {
			None => Err(Error::UnspecifiedQueryState),
			Some(policy_conf) => {
				let entropy_threshold = policy_conf
					.as_ref()
					.map(|conf| conf.entropy_threshold)
					.unwrap_or(10.0);

				let commit_percentage = policy_conf
					.as_ref()
					.map(|conf| conf.commit_percentage)
					.unwrap_or(0.0);

				Ok(format!(
					"(lte (divz (count (filter (gt {}) $)) (count $)) {})",
					entropy_threshold, commit_percentage
				))
			}
		}
	}

	fn explain_default_query(&self) -> Result<Option<String>> {
		Ok(Some(
			"entropy calculations of each commit in the repository".to_owned(),
		))
	}

	queries! {}
}

#[derive(Parser, Debug)]
struct Args {
	#[arg(long)]
	port: u16,

	#[arg(long, default_value_t=LogLevel::Error)]
	log_level: LogLevel,

	#[arg(trailing_var_arg(true), allow_hyphen_values(true), hide = true)]
	unknown_args: Vec<String>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
	let args = Args::try_parse().unwrap();
	PluginServer::register(EntropyPlugin::default(), args.log_level)
		.listen_local(args.port)
		.await
}
