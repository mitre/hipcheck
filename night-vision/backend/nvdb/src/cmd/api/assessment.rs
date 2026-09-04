//! Blocking client for container-executed package assessments.

use anyhow::{Context as _, Result, anyhow, bail};
use reqwest::{
    StatusCode,
    blocking::{Client, Response},
    header::{CONTENT_TYPE, USER_AGENT},
};
use serde::{Deserialize, Serialize};
use std::{
    thread,
    time::{Duration, Instant},
};
use uuid::Uuid;

const USER_AGENT_VALUE: &str = "nvdb";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Deserialize)]
pub struct AssessmentSubmission {
    pub id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct AssessmentStatus {
    pub id: Uuid,
    pub state: String,
    #[serde(rename = "sourceRepositoryUrl")]
    pub source_repository_url: Option<String>,
    pub recommendation: Option<String>,
    #[serde(rename = "findingCount")]
    pub finding_count: usize,
    #[serde(rename = "exitStatus")]
    pub exit_status: Option<i32>,
    #[serde(rename = "errorKind")]
    pub error_kind: Option<String>,
    #[serde(rename = "errorMessage")]
    pub error_message: Option<String>,
    pub retryable: Option<bool>,
}

#[derive(Serialize)]
struct AssessmentRequest<'a> {
    purl: &'a str,
}

/// Submit one assessment to the configured server container.
pub fn submit_assessment(
    client: &Client,
    server_url: &str,
    purl: &str,
) -> Result<AssessmentSubmission> {
    let url = format!("{server_url}/assessments");
    let response = client
        .post(&url)
        .header(USER_AGENT, USER_AGENT_VALUE)
        .header(CONTENT_TYPE, "application/json")
        .json(&AssessmentRequest { purl })
        .send()
        .with_context(|| format!("failed to submit assessment to {url}"))?;
    require_status(response, StatusCode::ACCEPTED, &url)?
        .json()
        .with_context(|| {
            format!("server returned an invalid assessment submission response from {url}")
        })
}

/// Build the client used for submitting and polling assessments.
pub fn client() -> Result<Client> {
    Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .context("failed to create assessment API client")
}

/// Poll a submitted assessment until the server records a terminal state.
pub fn wait_for_terminal_assessment(
    client: &Client,
    server_url: &str,
    id: Uuid,
    timeout: Duration,
) -> Result<AssessmentStatus> {
    let url = format!("{server_url}/assessments/{id}");
    let deadline = Instant::now()
        .checked_add(timeout)
        .context("assessment polling deadline overflowed")?;
    loop {
        let response = client
            .get(&url)
            .header(USER_AGENT, USER_AGENT_VALUE)
            .send()
            .with_context(|| format!("failed to poll assessment {id} from {url}"))?;
        let status: AssessmentStatus = require_status(response, StatusCode::OK, &url)?
            .json()
            .with_context(|| format!("server returned an invalid assessment status from {url}"))?;
        if matches!(status.state.as_str(), "completed" | "failed") {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            bail!(
                "timed out waiting for assessment {id} after {} ms",
                timeout.as_millis()
            );
        }
        thread::sleep(
            Duration::from_millis(250).min(deadline.saturating_duration_since(Instant::now())),
        );
    }
}

fn require_status(response: Response, expected: StatusCode, url: &str) -> Result<Response> {
    if response.status() == expected {
        return Ok(response);
    }
    let status = response.status();
    let body = response.text().unwrap_or_default();
    Err(anyhow!("{url} returned {status}: {body}"))
}

#[cfg(test)]
mod tests {
    use super::{submit_assessment, wait_for_terminal_assessment};
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
                .json_body_obj(&serde_json::json!({"purl": "pkg:npm/example@1.2.3"}));
            then.status(202)
                .header("content-type", "application/json")
                .body(format!(r#"{{"id":"{ASSESSMENT_ID}"}}"#));
        });

        let submitted = submit_assessment(
            &reqwest::blocking::Client::new(),
            &server.base_url(),
            "pkg:npm/example@1.2.3",
        )
        .expect("submission should succeed");

        assert_eq!(submitted.id, assessment_id());
        mock.assert();
    }

    #[test]
    fn returns_terminal_failed_status_with_diagnostics() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path(format!("/assessments/{ASSESSMENT_ID}"));
            then.status(200).header("content-type", "application/json").body(
                format!(r#"{{"id":"{ASSESSMENT_ID}","state":"failed","target":null,"sourceRepositoryUrl":null,"recommendation":null,"findingCount":0,"exitStatus":null,"errorKind":"start","errorMessage":"Hipcheck artifact is missing","retryable":false}}"#),
            );
        });

        let status = wait_for_terminal_assessment(
            &reqwest::blocking::Client::new(),
            &server.base_url(),
            assessment_id(),
            Duration::from_secs(1),
        )
        .expect("terminal response should succeed");

        assert_eq!(status.state, "failed");
        assert_eq!(status.error_kind.as_deref(), Some("start"));
        assert_eq!(
            status.error_message.as_deref(),
            Some("Hipcheck artifact is missing")
        );
        assert_eq!(status.retryable, Some(false));
        mock.assert();
    }

    #[test]
    fn rejects_an_unexpected_submission_status() {
        let server = MockServer::start();
        let _mock = server.mock(|when, then| {
            when.method(POST).path("/assessments");
            then.status(503).body("capacity exhausted");
        });

        let error = submit_assessment(
            &reqwest::blocking::Client::new(),
            &server.base_url(),
            "pkg:npm/example@1.2.3",
        )
        .expect_err("non-202 response must fail");

        assert!(error.to_string().contains("503 Service Unavailable"));
    }

    #[test]
    fn rejects_malformed_terminal_status() {
        let server = MockServer::start();
        let _mock = server.mock(|when, then| {
            when.method(GET)
                .path(format!("/assessments/{ASSESSMENT_ID}"));
            then.status(200)
                .header("content-type", "application/json")
                .body("not json");
        });

        let error = wait_for_terminal_assessment(
            &reqwest::blocking::Client::new(),
            &server.base_url(),
            assessment_id(),
            Duration::from_secs(1),
        )
        .expect_err("malformed status must fail");

        assert!(error.to_string().contains("invalid assessment status"));
    }

    #[test]
    fn times_out_while_assessment_is_non_terminal() {
        let server = MockServer::start();
        let _mock = server.mock(|when, then| {
            when.method(GET).path(format!("/assessments/{ASSESSMENT_ID}"));
            then.status(200).header("content-type", "application/json").body(
                format!(r#"{{"id":"{ASSESSMENT_ID}","state":"running","sourceRepositoryUrl":null,"recommendation":null,"findingCount":0,"exitStatus":null,"errorKind":null,"errorMessage":null,"retryable":null}}"#),
            );
        });

        let error = wait_for_terminal_assessment(
            &reqwest::blocking::Client::new(),
            &server.base_url(),
            assessment_id(),
            Duration::ZERO,
        )
        .expect_err("non-terminal status must time out");

        assert!(
            error
                .to_string()
                .contains("timed out waiting for assessment 0198f30e-2bfa-7000-8000-000000000007")
        );
    }
}
