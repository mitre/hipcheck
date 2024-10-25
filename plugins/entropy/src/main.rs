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
struct Config {
	#[serde(rename = "langs-file")]
	langs_file: Option<PathBuf>,
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

#[derive(Clone, Debug)]
struct EntropyPlugin;

impl Plugin for EntropyPlugin {
	const PUBLISHER: &'static str = "mitre";
	const NAME: &'static str = "entropy";
	fn set_config(&self, config: Value) -> StdResult<(), ConfigError> {
		let conf: Config =
			serde_json::from_value(config).map_err(|e| ConfigError::Unspecified {
				message: e.to_string(),
			})?;
		let sfd = match conf.langs_file {
			Some(p) => SourceFileDetector::load(p).map_err(|e| ConfigError::Unspecified {
				message: e.to_string(),
			})?,
			None => {
				return Err(ConfigError::MissingRequiredConfig {
					field_name: "langs-file".to_owned(),
					field_type: "string".to_owned(),
					possible_values: vec![],
				});
			}
		};
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
		Ok("".to_owned())
	}

	fn explain_default_query(&self) -> Result<Option<String>> {
		Ok(None)
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
	PluginServer::register(EntropyPlugin {})
		.listen(args.port)
		.await
}
