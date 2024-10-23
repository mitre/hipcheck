// SPDX-License-Identifier: Apache-2.0

//! Plugin for querying what percentage of pull requests were merged without review

use clap::Parser;
use hipcheck_sdk::{prelude::*, types::Target};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{result::Result as StdResult, sync::OnceLock};

#[derive(Deserialize)]
struct Config {
	percent_threshold: Option<f64>,
}

static CONFIG: OnceLock<Config> = OnceLock::new();

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct PullRequest {
	pub id: u64,
	pub reviews: u64,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PullReview {
	pub pull_request: PullRequest,
	pub has_review: bool,
}

/// Returns the percentage of commits to the repo that were merged without review
#[query]
async fn review(engine: &mut PluginEngine, value: Target) -> Result<f64> {
	log::debug!("running review metric");

	// Confirm that the target is a GitHub repo
	let Some(remote) = value.remote else {
		log::error!("target repository does not have a remote repository URL");
		return Err(Error::UnexpectedPluginQueryInputFormat);
	};
	let Some(known_remote) = remote.known_remote else {
		log::error!("target repository is not a GitHub repository or else is missing GitHub repo information");
		return Err(Error::UnexpectedPluginQueryInputFormat);
	};

	// Get a list of all pull requests to the repo, with their corresponding number of reviews
	let value = engine
		.query("mitre/github_api/pr_reviews", known_remote)
		.await
		.map_err(|e| {
			log::error!(
				"failed to get pull request reviews from GitHub for review query: {}",
				e
			);
			Error::UnspecifiedQueryState
		})?;

	let pull_requests: Vec<PullRequest> =
		serde_json::from_value(value).map_err(Error::InvalidJsonInQueryOutput)?;

	log::trace!("got pull requests [requests='{:#?}']", pull_requests);

	// Create a Vec big enough to hold every single pull request
	let mut pull_reviews = Vec::with_capacity(pull_requests.len());

	// Create a list of pull requesets, with a boolean indicating if they have at least one review or not
	for pull_request in pull_requests {
		let has_review = pull_request.reviews > 0;
		pull_reviews.push(PullReview {
			pull_request,
			has_review,
		});
	}

	// Calculate the percentage of unreviewed pull requests out of the total
	// If there are no pull requests, return 0.0 to avoid a divide-by-zero error
	let num_flagged = pull_reviews.iter().filter(|p| !p.has_review).count() as u64;
	let percent_flagged = match (num_flagged, pull_reviews.len()) {
		(flagged, total) if flagged != 0 && total != 0 => {
			num_flagged as f64 / pull_reviews.len() as f64
		}
		_ => 0.0,
	};

	log::info!("completed review query");

	Ok(percent_flagged)
}

#[derive(Clone, Debug)]
struct ReviewPlugin;

impl Plugin for ReviewPlugin {
	const PUBLISHER: &'static str = "mitre";

	const NAME: &'static str = "review";

	fn set_config(&self, config: Value) -> StdResult<(), ConfigError> {
		let conf =
			serde_json::from_value::<Config>(config).map_err(|e| ConfigError::Unspecified {
				message: e.to_string(),
			})?;
		CONFIG.set(conf).map_err(|_e| ConfigError::Unspecified {
			message: "config was already set".to_owned(),
		})
	}

	fn default_policy_expr(&self) -> Result<String> {
		let Some(conf) = CONFIG.get() else {
			log::error!("tried to access config before set by Hipcheck core!");
			return Err(Error::UnspecifiedQueryState);
		};
		match conf.percent_threshold {
			Some(threshold) => Ok(format!("lte $ {}", threshold)),
			None => Ok("".to_owned()),
		}
	}

	fn explain_default_query(&self) -> Result<Option<String>> {
		Ok(Some(
			"Percentage of unreviewed commits to the repo".to_string(),
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
	PluginServer::register(ReviewPlugin {})
		.listen(args.port)
		.await
}

#[cfg(test)]
mod test {
	use super::*;

	use hipcheck_sdk::types::{KnownRemote, LocalGitRepo, RemoteGitRepo};
	use std::result::Result as StdResult;
	use url::Url;

	fn known_remote() -> KnownRemote {
		KnownRemote::GitHub {
			owner: "expresjs".to_string(),
			repo: "express".to_string(),
		}
	}

	fn mock_responses() -> StdResult<MockResponses, Error> {
		let known_remote = known_remote();

		let pr1 = PullRequest { id: 1, reviews: 1 };
		let pr2 = PullRequest { id: 2, reviews: 3 };
		let pr3 = PullRequest { id: 3, reviews: 0 };
		let pr4 = PullRequest { id: 4, reviews: 1 };
		let prs = vec![pr1, pr2, pr3, pr4];

		// when calling into query, the input known_remote gets passed to `pr_reviews`, lets assume it returns the vec of PullRequests `prs`
		let mut mock_responses = MockResponses::new();
		mock_responses.insert("mitre/github_api/pr_reviews", known_remote, Ok(prs))?;
		Ok(mock_responses)
	}

	#[tokio::test]
	async fn test_activity() {
		let target = Target {
			specifier: "express".to_string(),
			local: LocalGitRepo {
				path: "/home/users/me/.cache/hipcheck/clones/github/expressjs/express/".to_string(),
				git_ref: "main".to_string(),
			},
			remote: Some(RemoteGitRepo {
				url: Url::parse("https://github.com/expressjs/express.git").unwrap(),
				known_remote: Some(known_remote()),
			}),
			package: None,
		};

		let mut engine = PluginEngine::mock(mock_responses().unwrap());
		let result = review(&mut engine, target).await.unwrap();

		let expected = 0.25;

		assert_eq!(result, expected);
	}
}
