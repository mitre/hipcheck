use anyhow::{Context as _, Result};
use nv_common::{config::Config, rt::AsyncRuntime};
use std::time::Duration;

use crate::cmd::api::assessment::{submit_assessment, wait_for_terminal_assessment};
use crate::cmd::api::client as api_client;

const ASSESSMENT_WAIT_GRACE: Duration = Duration::from_secs(30);

pub fn command() -> clap::Command {
	clap::Command::new("analyze")
		.about("Run the configured assessment policy for an explicit upgrade")
		.arg(affected_purl_argument())
		.arg(target_purl_argument())
		.arg(json_argument())
}

pub fn run(config: &Config, matches: &clap::ArgMatches) -> Result<()> {
	let affected_purl = matches
		.get_one::<String>("affected-purl")
		.expect("required PURL");
	let target_purl = matches
		.get_one::<String>("target-purl")
		.expect("required PURL");
	eprintln!("queuing assessment for upgrade from {affected_purl} to {target_purl}");
	let runtime = AsyncRuntime::new(config).context("failed to create async runtime")?;
	let client = api_client(&format!("http://{}", config.server_address))?;
	let submitted = runtime.block_on(submit_assessment(&client, affected_purl, target_purl))?;
	eprintln!(
		"running configured Hipcheck policy for assessment {}",
		submitted.id
	);
	let wait_timeout = Duration::from_millis(config.hipcheck_timeout)
		.checked_add(ASSESSMENT_WAIT_GRACE)
		.context("assessment wait timeout overflowed")?;
	let assessment = runtime.block_on(wait_for_terminal_assessment(
		&client,
		submitted.id,
		wait_timeout,
	))?;
	let persisted_affected_purl = assessment.affected_purl.as_deref().unwrap_or(affected_purl);
	eprintln!("completed assessment {}", assessment.id);
	if matches.get_flag("json") {
		println!(
			"{}",
			json_output(
				assessment.id,
				persisted_affected_purl,
				target_purl,
				&assessment.state,
				assessment.recommendation.as_deref(),
				assessment.finding_count,
				assessment.error_kind.as_deref(),
				assessment.error_message.as_deref(),
				assessment.exit_status,
				assessment.source_repository_url.as_deref(),
				assessment.retryable,
			)
		);
	} else {
		println!(
			"assessment {}\naffected_purl: {}\ntarget: {}\nstate: {}\nrecommendation: {}\nfindings: {}{}",
			assessment.id,
			persisted_affected_purl,
			target_purl,
			assessment.state,
			assessment.recommendation.as_deref().unwrap_or("n/a"),
			assessment.finding_count,
			failed_diagnostics(
				&assessment.state,
				assessment.error_kind.as_deref(),
				assessment.error_message.as_deref(),
				assessment.exit_status,
				assessment.source_repository_url.as_deref(),
				assessment.retryable
			),
		);
	}
	Ok(())
}

#[expect(
	clippy::too_many_arguments,
	reason = "the CLI output preserves the flat assessment status response"
)]
fn json_output(
	id: uuid::Uuid,
	affected_purl: &str,
	target_purl: &str,
	state: &str,
	recommendation: Option<&str>,
	finding_count: u32,
	error_kind: Option<&str>,
	error_message: Option<&str>,
	exit_status: Option<i32>,
	source_repository_url: Option<&str>,
	retryable: Option<bool>,
) -> serde_json::Value {
	serde_json::json!({
		"id": id,
		"affectedPurl": affected_purl,
		"target": target_purl,
		"state": state,
		"recommendation": recommendation,
		"findingCount": finding_count,
		"errorKind": error_kind,
		"errorMessage": error_message,
		"exitStatus": exit_status,
		"sourceRepositoryUrl": source_repository_url,
		"retryable": retryable,
	})
}

fn failed_diagnostics(
	state: &str,
	error_kind: Option<&str>,
	error_message: Option<&str>,
	exit_status: Option<i32>,
	source_repository_url: Option<&str>,
	retryable: Option<bool>,
) -> String {
	if state != "failed" {
		return String::new();
	}
	format!(
		"\nsource_repository_url: {}\nexit_status: {}\nerror_kind: {}\nerror_message: {}\nretryable: {}",
		source_repository_url.unwrap_or("n/a"),
		exit_status.map_or_else(|| "n/a".to_owned(), |value| value.to_string()),
		error_kind.unwrap_or("n/a"),
		error_message.unwrap_or("n/a"),
		retryable.map_or_else(|| "n/a".to_owned(), |value| value.to_string())
	)
}

fn affected_purl_argument() -> clap::Arg {
	clap::Arg::new("affected-purl")
		.required(true)
		.value_name("AFFECTED-PURL")
		.help("KEV-affected package URL to upgrade from")
}
fn target_purl_argument() -> clap::Arg {
	clap::Arg::new("target-purl")
		.required(true)
		.value_name("TARGET-PURL")
		.help("Eligible newer package URL to assess")
}

fn json_argument() -> clap::Arg {
	clap::Arg::new("json")
		.long("json")
		.action(clap::ArgAction::SetTrue)
		.help("Print the persisted assessment summary as JSON")
}

#[cfg(test)]
mod tests {
	use super::{command, failed_diagnostics, json_output};
	use uuid::Uuid;

	const ASSESSMENT_ID: &str = "0198f30e-2bfa-7000-8000-000000000007";

	#[test]
	fn analyze_accepts_upgrade_purls() {
		command()
			.try_get_matches_from([
				"analyze",
				"pkg:npm/example@1.2.3",
				"pkg:npm/example@1.2.4",
				"--json",
			])
			.expect("assessment analyze should parse");
	}

	#[test]
	fn analyze_json_output_preserves_the_completed_assessment_summary() {
		assert_eq!(
			json_output(
				Uuid::parse_str(ASSESSMENT_ID).expect("assessment ID must be valid"),
				"pkg:npm/example@1.2.3",
				"pkg:npm/example@1.2.4",
				"completed",
				Some("pass"),
				2,
				None,
				None,
				None,
				None,
				None,
			),
			serde_json::json!({
				"id": ASSESSMENT_ID,
				"affectedPurl": "pkg:npm/example@1.2.3",
				"target": "pkg:npm/example@1.2.4",
				"state": "completed",
				"recommendation": "pass",
				"findingCount": 2,
				"errorKind": null,
				"errorMessage": null,
				"exitStatus": null,
				"sourceRepositoryUrl": null,
				"retryable": null,
			})
		);
	}

	#[test]
	fn failed_text_diagnostics_include_retryability() {
		assert_eq!(
			failed_diagnostics(
				"failed",
				Some("start"),
				Some("artifact missing"),
				Some(127),
				Some("https://github.com/example/project"),
				Some(false)
			),
			"\nsource_repository_url: https://github.com/example/project\nexit_status: 127\nerror_kind: start\nerror_message: artifact missing\nretryable: false"
		);
	}
}
