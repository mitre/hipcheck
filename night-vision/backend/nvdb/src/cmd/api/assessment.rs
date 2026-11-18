//! Typed API helpers for the assessment workflow.

use anyhow::{Context as _, Result, anyhow, bail};
use nv_server_client::{Client, types};
use std::time::{Duration, Instant};
use uuid::Uuid;

pub type AssessmentStatus = types::AssessmentStatus;
pub type AssessmentSubmission = types::PostAssessmentResponse;

/// Submit one assessment to the configured server.
pub async fn submit_assessment(
	client: &Client,
	affected_purl: &str,
	target_purl: &str,
) -> Result<AssessmentSubmission> {
	client
		.post_assessment()
		.body(types::PostAssessmentBody {
			affected_purl: affected_purl.to_owned(),
			target_purl: target_purl.to_owned(),
		})
		.send()
		.await
		.map(nv_server_client::ResponseValue::into_inner)
		.map_err(|error| anyhow!("POST /assessments failed: {error}"))
}

/// Poll a submitted assessment until the server records a terminal state.
pub async fn wait_for_terminal_assessment(
	client: &Client,
	id: Uuid,
	timeout: Duration,
) -> Result<AssessmentStatus> {
	let deadline = Instant::now()
		.checked_add(timeout)
		.context("assessment polling deadline overflowed")?;
	loop {
		let status = client
			.get_assessment()
			.id(id)
			.send()
			.await
			.map(nv_server_client::ResponseValue::into_inner)
			.map_err(|error| anyhow!("GET /assessments/{id} failed: {error}"))?;
		if matches!(status.state.as_str(), "completed" | "failed") {
			return Ok(status);
		}
		if Instant::now() >= deadline {
			bail!(
				"timed out waiting for assessment {id} after {} ms",
				timeout.as_millis()
			);
		}
		tokio::time::sleep(
			Duration::from_millis(250).min(deadline.saturating_duration_since(Instant::now())),
		)
		.await;
	}
}

#[cfg(test)]
mod tests {
	use super::{submit_assessment, wait_for_terminal_assessment};
	use crate::cmd::api::client;
	use httpmock::prelude::*;
	use std::time::Duration;
	use uuid::Uuid;

	const ASSESSMENT_ID: &str = "0198f30e-2bfa-7000-8000-000000000007";

	fn assessment_id() -> Uuid {
		Uuid::parse_str(ASSESSMENT_ID).expect("assessment ID must be valid")
	}

	#[test]
	fn submits_json_assessment_request() {
		let server = MockServer::start();
		let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/assessments")
                .header("content-type", "application/json")
                .header("user-agent", "nvdb")
                .json_body_obj(&serde_json::json!({"affectedPurl": "pkg:npm/example@1.2.3", "targetPurl": "pkg:npm/example@1.2.4"}));
            then.status(202).header("content-type", "application/json").body(format!(r#"{{"id":"{ASSESSMENT_ID}"}}"#));
        });
		let runtime = tokio::runtime::Runtime::new().expect("runtime");
		let submitted = runtime
			.block_on(submit_assessment(
				&client(&server.base_url()).expect("client"),
				"pkg:npm/example@1.2.3",
				"pkg:npm/example@1.2.4",
			))
			.expect("submission succeeds");
		assert_eq!(submitted.id, assessment_id());
		mock.assert();
	}

	#[test]
	fn returns_terminal_failed_status_with_diagnostics() {
		let server = MockServer::start();
		let mock = server.mock(|when, then| {
            when.method(GET).path(format!("/assessments/{ASSESSMENT_ID}"));
            then.status(200).header("content-type", "application/json").body(format!(r#"{{"id":"{ASSESSMENT_ID}","state":"failed","target":null,"sourceRepositoryUrl":null,"recommendation":null,"findingCount":0,"exitStatus":null,"errorKind":"start","errorMessage":"Hipcheck artifact is missing","retryable":false}}"#));
        });
		let runtime = tokio::runtime::Runtime::new().expect("runtime");
		let api_client = client(&server.base_url()).expect("client");
		let status = runtime
			.block_on(wait_for_terminal_assessment(
				&api_client,
				assessment_id(),
				Duration::from_secs(1),
			))
			.expect("terminal status");
		assert_eq!(status.state, "failed");
		assert_eq!(status.error_kind.as_deref(), Some("start"));
		mock.assert();
	}
}
