// SPDX-License-Identifier: Apache-2.0

mod error;
mod linguist;
mod metric;
mod types;
mod util;

use crate::{
	metric::*,
	types::{CommitChurn, CommitChurnFreq, CommitDiff},
	util::db::*,
};
use clap::Parser;
use hipcheck_sdk::{prelude::*, types::Target};
use linguist::SourceFileDetector;
use serde::Deserialize;
use std::{
	collections::HashMap,
	path::PathBuf,
	result::Result as StdResult,
	sync::{Arc, OnceLock},
};
use tokio::sync::Mutex;

#[derive(Deserialize)]
struct RawConfig {
	#[serde(rename = "langs-file")]
	langs_file: Option<PathBuf>,
	#[serde(rename = "churn-freq")]
	churn_freq: Option<f64>,
	#[serde(rename = "commit-percentage")]
	commit_percentage: Option<f64>,
}

#[derive(Clone, Debug)]
struct PolicyExprConf {
	pub churn_freq: f64,
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
				field_name: "langs_file".to_owned(),
				field_type: "string".to_owned(),
				possible_values: vec![],
			});
		};
		// Default policy expr depends on two fields. If neither present, no default
		// policy. else make sure both are present
		let opt_policy = match (value.churn_freq, value.commit_percentage) {
			(None, None) => None,
			(Some(_), None) => {
				return Err(ConfigError::MissingRequiredConfig {
					field_name: "commit_percentage".to_owned(),
					field_type: "float".to_owned(),
					possible_values: vec![],
				});
			}
			(None, Some(_)) => {
				return Err(ConfigError::MissingRequiredConfig {
					field_name: "churn_freq".to_owned(),
					field_type: "float".to_owned(),
					possible_values: vec![],
				});
			}
			(Some(churn_freq), Some(commit_percentage)) => Some(PolicyExprConf {
				churn_freq,
				commit_percentage,
			}),
		};
		// Sanity check on policy expr config
		if let Some(policy_ref) = &opt_policy {
			if policy_ref.commit_percentage < 0.0 || policy_ref.commit_percentage > 1.0 {
				return Err(ConfigError::InvalidConfigValue {
					field_name: "commit_percentage".to_owned(),
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
async fn commit_churns(
	_engine: &mut PluginEngine,
	mut commit_diffs: Vec<CommitDiff>,
) -> Result<Vec<CommitChurnFreq>> {
	let linguist = DATABASE
		.get()
		.ok_or(Error::UnspecifiedQueryState)?
		.lock()
		.await;
	commit_diffs.retain(|x| is_likely_source_file_cd(&linguist, x));

	let mut commit_churns = Vec::new();
	let mut total_files_changed: i64 = 0;
	let mut total_lines_changed: i64 = 0;

	for commit_diff in commit_diffs {
		let source_files = commit_diff
			.diff
			.file_diffs
			.iter()
			.filter(|file_diff| linguist.is_likely_source_file(file_diff.file_name.clone()))
			.collect::<Vec<_>>();

		// Update files changed.
		let files_changed = source_files.len() as i64;
		total_files_changed += files_changed;

		// Update lines changed.
		let mut lines_changed: i64 = 0;
		for file_diff in &source_files {
			lines_changed += file_diff.additions.ok_or_else(|| {
				log::error!("GitHub commits can't be used for churn");
				Error::UnspecifiedQueryState
			})?;
			lines_changed += file_diff.deletions.ok_or_else(|| {
				log::error!("GitHub commits can't be used for churn");
				Error::UnspecifiedQueryState
			})?;
		}
		total_lines_changed += lines_changed;

		commit_churns.push(CommitChurn {
			commit: commit_diff.commit.clone(),
			files_changed,
			lines_changed,
		});
	}

	let mut commit_churn_freqs: Vec<_> = {
		let file_frequencies: HashMap<&str, f64> = commit_churns
			.iter()
			.map(|commit_churn| {
				// avoid dividing by zero.
				if total_files_changed == 0 {
					(commit_churn.commit.hash.as_ref(), 0.0)
				} else {
					(
						commit_churn.commit.hash.as_ref(),
						commit_churn.files_changed as f64 / total_files_changed as f64,
					)
				}
			})
			.collect();

		let line_frequencies: HashMap<&str, f64> = commit_churns
			.iter()
			.map(|commit_churn| {
				// avoid dividing by zero.
				if total_lines_changed == 0 {
					(commit_churn.commit.hash.as_ref(), 0.0)
				} else {
					(
						commit_churn.commit.hash.as_ref(),
						commit_churn.lines_changed as f64 / total_lines_changed as f64,
					)
				}
			})
			.collect();

		commit_churns
			.iter()
			.map(|commit_churn| {
				let hash: &str = commit_churn.commit.hash.as_ref();
				let file_frequency = file_frequencies[hash];
				let line_frequency = line_frequencies[hash];
				// PANIC: Safe to unwrap, beacuse we are creating a valid floating point number
				let churn = file_frequency * line_frequency * line_frequency * 1_000_000.0;
				CommitChurnFreq {
					commit: commit_churn.commit.clone(),
					churn,
				}
			})
			.collect()
	};

	let churns: Vec<_> = commit_churn_freqs.iter().map(|c| c.churn).collect();

	let mean = mean(&churns).ok_or_else(|| {
		log::error!("failed to get mean churn value");
		Error::UnspecifiedQueryState
	})?;
	let std_dev = std_dev(mean, &churns).ok_or_else(|| {
		log::error!("failed to get churn standard deviation");
		Error::UnspecifiedQueryState
	})?;

	log::trace!("mean of churn scores [mean='{}']", mean);
	log::trace!("standard deviation of churn scores [stddev='{}']", std_dev);

	if std_dev == 0.0 {
		log::error!("not enough commits to calculate churn");
		return Err(Error::UnspecifiedQueryState);
	}

	for commit_churn_freq in &mut commit_churn_freqs {
		commit_churn_freq.churn = (commit_churn_freq.churn - mean) / std_dev;
	}

	log::info!("completed churn metric");

	Ok(commit_churn_freqs)
}

#[query(default)]
async fn churn(engine: &mut PluginEngine, value: Target) -> Result<Vec<f64>> {
	let local = value.local;
	let val_commits = engine.query("mitre/git/commit_diffs", local).await?;
	let commits: Vec<CommitDiff> =
		serde_json::from_value(val_commits).map_err(Error::InvalidJsonInQueryOutput)?;
	Ok(commit_churns(engine, commits)
		.await?
		.iter()
		.map(|o| o.churn)
		.collect())
}

#[derive(Clone, Debug, Default)]
struct ChurnPlugin {
	policy_conf: OnceLock<Option<PolicyExprConf>>,
}

impl Plugin for ChurnPlugin {
	const PUBLISHER: &'static str = "mitre";
	const NAME: &'static str = "churn";

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

		// Use the langs file to create a SourceFileDetector and init the salsa db
		let sfd =
			SourceFileDetector::load(conf.langs_file).map_err(|e| ConfigError::Unspecified {
				message: e.to_string(),
			})?;

		let mut database = Linguist::new();
		database.set_source_file_detector(Arc::new(sfd));
		let global_db = Arc::new(Mutex::new(database));

		// Make the salsa db globally accessible
		DATABASE
			.set(global_db)
			.map_err(|_e| ConfigError::Unspecified {
				message: "config was already set".to_owned(),
			})
	}

	fn default_policy_expr(&self) -> Result<String> {
		match self.policy_conf.get() {
			None => Err(Error::UnspecifiedQueryState),
			Some(policy_conf) => {
				let churn_freq = policy_conf
					.as_ref()
					.map(|conf| conf.churn_freq)
					.unwrap_or(3.0);

				let commit_percentage = policy_conf
					.as_ref()
					.map(|conf| conf.commit_percentage)
					.unwrap_or(0.02);

				Ok(format!(
					"(lte (divz (count (filter (gt {}) $)) (count $)) {})",
					churn_freq, commit_percentage
				))
			}
		}
	}

	fn explain_default_query(&self) -> Result<Option<String>> {
		Ok(Some(
			"the churn frequency of each commit in the repository".to_owned(),
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
	PluginServer::register(ChurnPlugin::default())
		.listen(args.port)
		.await
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::types::{Commit, Diff, FileDiff};
	use pathbuf::pathbuf;

	fn init_db_if_uninited() {
		fn create_db() -> Arc<Mutex<Linguist>> {
			let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
			let langs_path = pathbuf![&manifest_dir, "../../config/Langs.kdl"];
			let sfd = SourceFileDetector::load(langs_path)
				.map_err(|e| ConfigError::Unspecified {
					message: e.to_string(),
				})
				.unwrap();
			let mut database = Linguist::new();
			database.set_source_file_detector(Arc::new(sfd));
			Arc::new(Mutex::new(database))
		}
		DATABASE.get_or_init(create_db);
	}

	fn test_data() -> Vec<CommitDiff> {
		let c1 = Commit {
			hash: "abc123".to_owned(),
			written_on: Ok("10/23/2024".to_owned()),
			committed_on: Ok("10/23/2024".to_owned()),
		};
		let c2 = Commit {
			hash: "def456".to_owned(),
			written_on: Ok("10/23/2024".to_owned()),
			committed_on: Ok("10/23/2024".to_owned()),
		};
		let d1 = Diff {
			additions: Some(100),
			deletions: Some(20),
			file_diffs: vec![
				FileDiff {
					file_name: "foo.java".to_owned(),
					additions: Some(80),
					deletions: Some(0),
					patch: "".to_owned(),
				},
				FileDiff {
					file_name: "bar.java".to_owned(),
					additions: Some(10),
					deletions: Some(15),
					patch: "".to_owned(),
				},
				FileDiff {
					file_name: "baz.java".to_owned(),
					additions: Some(10),
					deletions: Some(5),
					patch: "".to_owned(),
				},
			],
		};
		let d2 = Diff {
			additions: Some(2000),
			deletions: Some(1500),
			file_diffs: vec![
				FileDiff {
					file_name: "foo.java".to_owned(),
					additions: Some(100),
					deletions: Some(1200),
					patch: "".to_owned(),
				},
				FileDiff {
					file_name: "bar.java".to_owned(),
					additions: Some(1800),
					deletions: Some(300),
					patch: "".to_owned(),
				},
				FileDiff {
					file_name: "baz.java".to_owned(),
					additions: Some(100),
					deletions: Some(0),
					patch: "".to_owned(),
				},
			],
		};

		vec![
			CommitDiff {
				commit: c1,
				diff: d1,
			},
			CommitDiff {
				commit: c2,
				diff: d2,
			},
		]
	}

	#[tokio::test]
	async fn test_foo() {
		init_db_if_uninited();

		let mut engine = PluginEngine::mock(MockResponses::new());
		let key = test_data();

		let freqs = commit_churns(&mut engine, key).await.unwrap();

		// Churn metric normalizes across the mean and returns churns as
		// standard deviations from the mean. Since we have only two values,
		// the mean will always be halfway between the two and one will be
		// one std dev less, the other one std dev more
		assert_eq!(freqs.len(), 2);
		assert_eq!(freqs[0].churn, -1.0);
		assert_eq!(freqs[1].churn, 1.0);
	}
}
