//! Hipcheck JSON consumer contract for Night Vision.
//!
//! This module intentionally models only the backend-facing contract described in:
//! - docs/rfds/0002-use-hipcheck-for-supply-chain-analysis.md
//! - docs/backend/hipcheck-integration.md
//!
//! The model is intentionally small:
//! - keep structured fields used for normalization
//! - reject missing required identity/state/value fields
//! - allow optional evidence fields to be absent
//! - do not depend on display text

use serde::Deserialize;
use serde_json::{Map, Value};
use std::{error::Error, fmt};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct HipcheckReport {
    pub schema_version: String,
    #[serde(rename = "hipcheck")]
    pub hipcheck: HipcheckBuild,
    pub target: HipcheckTarget,
    pub policy: HipcheckPolicy,
    pub checks: Vec<HipcheckCheck>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct HipcheckBuild {
    pub version: String,
    pub commit: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct HipcheckTarget {
    pub kind: String,
    #[serde(default)]
    pub purl: Option<String>,
    pub source_repository_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct HipcheckPolicy {
    pub id: String,
    #[serde(default)]
    pub version: Option<String>,
    pub source: String,
    pub recommendation: HipcheckRecommendation,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HipcheckRecommendation {
    Pass,
    Investigate,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct HipcheckCheck {
    pub plugin: HipcheckPluginIdentity,
    pub policy: HipcheckCheckPolicy,
    pub state: HipcheckCheckState,
    pub effect: HipcheckEffect,
    #[serde(default)]
    pub severity: Option<String>,
    pub summary: String,
    pub value: Value,
    #[serde(default)]
    pub concerns: Vec<HipcheckConcern>,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub ended_at: Option<String>,
    #[serde(default)]
    pub error: Option<HipcheckError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct HipcheckPluginIdentity {
    pub name: String,
    pub publisher: String,
    pub version: String,
    pub query: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct HipcheckCheckPolicy {
    pub expression: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HipcheckCheckState {
    Passed,
    Failed,
    Skipped,
    Unsupported,
    Errored,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HipcheckEffect {
    Blocking,
    Review,
    Context,
    MissingCheck,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct HipcheckConcern {
    pub kind: String,
    pub message: String,
    #[serde(default)]
    pub details: Option<Map<String, Value>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct HipcheckError {
    pub kind: String,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug)]
pub enum HipcheckReportError {
    Json(serde_json::Error),
    Contract { detail: &'static str },
}

impl From<serde_json::Error> for HipcheckReportError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl fmt::Display for HipcheckReportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(error) => write!(formatter, "invalid Hipcheck report JSON: {error}"),
            Self::Contract { detail } => {
                write!(formatter, "invalid Hipcheck report contract: {detail}")
            }
        }
    }
}

impl Error for HipcheckReportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Json(error) => Some(error),
            Self::Contract { .. } => None,
        }
    }
}

impl HipcheckReportError {
    fn contract(detail: &'static str) -> Self {
        Self::Contract { detail }
    }

    pub fn detail(&self) -> Option<&'static str> {
        match self {
            Self::Contract { detail } => Some(detail),
            Self::Json(_) => None,
        }
    }
}

/// Parses a Hipcheck report and enforces the backend consumer contract.
pub fn parse_hipcheck_report(input: &str) -> Result<HipcheckReport, HipcheckReportError> {
    let report: HipcheckReport = serde_json::from_str(input)?;
    validate_report(&report)?;
    Ok(report)
}

fn validate_report(report: &HipcheckReport) -> Result<(), HipcheckReportError> {
    if report.target.kind == "npm"
        && !report
            .target
            .purl
            .as_deref()
            .is_some_and(has_npm_package_version)
    {
        return Err(HipcheckReportError::contract(
            "target.purl must be a versioned npm package URL for npm targets",
        ));
    }

    for check in &report.checks {
        validate_timestamp(check.started_at.as_deref(), "checks[].started_at")?;
        validate_timestamp(check.ended_at.as_deref(), "checks[].ended_at")?;
    }

    Ok(())
}

fn has_npm_package_version(purl: &str) -> bool {
    let Some(package_and_version) = purl.strip_prefix("pkg:npm/") else {
        return false;
    };
    let package_and_version = package_and_version
        .split_once(['?', '#'])
        .map_or(package_and_version, |(path, _)| path);

    package_and_version
        .rsplit_once('@')
        .is_some_and(|(package, version)| !package.is_empty() && !version.is_empty())
}

fn validate_timestamp(
    timestamp: Option<&str>,
    field: &'static str,
) -> Result<(), HipcheckReportError> {
    if timestamp.is_some_and(|value| value.parse::<jiff::Timestamp>().is_err()) {
        return Err(HipcheckReportError::contract(match field {
            "checks[].started_at" => "checks[].started_at must be an RFC 3339 timestamp",
            "checks[].ended_at" => "checks[].ended_at must be an RFC 3339 timestamp",
            _ => unreachable!("only documented timestamp fields are validated"),
        }));
    }

    Ok(())
}
