//! Hipcheck 3.15 JSON normalization for Night Vision.
//!
//! Hipcheck's native JSON is retained as audit evidence. This module converts
//! its stable result fields plus trusted Night Vision assessment context into
//! the richer structure used by persistence and API normalization.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{error::Error, fmt};
use url::Url;

const NORMALIZED_SCHEMA_VERSION: &str = "1";
const NIGHT_VISION_POLICY_ID: &str = "night-vision-upgrade-assessment";
const UNKNOWN: &str = "unknown";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HipcheckReport {
    pub schema_version: String,
    pub hipcheck: HipcheckBuild,
    pub target: HipcheckTarget,
    pub policy: HipcheckPolicy,
    pub checks: Vec<HipcheckCheck>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HipcheckBuild {
    pub version: String,
    pub commit: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HipcheckTarget {
    pub kind: String,
    pub purl: Option<String>,
    pub source_repository_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HipcheckPolicy {
    pub id: String,
    pub version: Option<String>,
    pub source: String,
    pub recommendation: HipcheckRecommendation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HipcheckRecommendation {
    Pass,
    Investigate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HipcheckCheck {
    pub plugin: HipcheckPluginIdentity,
    pub policy: HipcheckCheckPolicy,
    pub state: HipcheckCheckState,
    pub effect: HipcheckEffect,
    pub severity: Option<String>,
    pub summary: String,
    pub value: Value,
    pub concerns: Vec<HipcheckConcern>,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub error: Option<HipcheckError>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HipcheckPluginIdentity {
    pub name: String,
    pub publisher: String,
    pub version: String,
    pub query: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HipcheckCheckPolicy {
    pub expression: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HipcheckCheckState {
    Passed,
    Failed,
    Skipped,
    Unsupported,
    Errored,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HipcheckEffect {
    Blocking,
    Review,
    Context,
    MissingCheck,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HipcheckConcern {
    pub kind: String,
    pub message: String,
    pub details: Option<Map<String, Value>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HipcheckError {
    pub kind: String,
    pub message: String,
    pub retryable: bool,
}

/// Trusted context that Hipcheck 3.15 does not include in its native report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HipcheckReportContext {
    pub target_purl: String,
    pub source_repository_url: String,
    pub policy_source: String,
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
            Self::Json(_) => formatter.write_str("invalid Hipcheck report JSON"),
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

/// Parse native Hipcheck 3.15 JSON into Night Vision's normalized report.
pub fn parse_hipcheck_report(
    input: &str,
    context: &HipcheckReportContext,
) -> Result<HipcheckReport, HipcheckReportError> {
    let native: NativeHipcheckReport = serde_json::from_str(input)?;
    validate_context(context)?;
    let recommendation = match native.recommendation.kind {
        NativeRecommendationKind::Pass => HipcheckRecommendation::Pass,
        NativeRecommendationKind::Investigate => HipcheckRecommendation::Investigate,
    };
    let mut checks = Vec::new();
    checks.extend(
        native.passing.into_iter().map(|check| {
            normalize_check(check, HipcheckCheckState::Passed, HipcheckEffect::Context)
        }),
    );
    checks.extend(
        native.failing.into_iter().map(|check| {
            normalize_check(check, HipcheckCheckState::Failed, HipcheckEffect::Review)
        }),
    );
    checks.extend(native.errored.into_iter().map(|check| {
        normalize_check(
            check,
            HipcheckCheckState::Errored,
            HipcheckEffect::MissingCheck,
        )
    }));
    Ok(HipcheckReport {
        schema_version: NORMALIZED_SCHEMA_VERSION.to_owned(),
        hipcheck: HipcheckBuild {
            version: native.hipcheck_version,
            commit: UNKNOWN.to_owned(),
        },
        target: HipcheckTarget {
            kind: "npm".to_owned(),
            purl: Some(context.target_purl.clone()),
            source_repository_url: context.source_repository_url.clone(),
        },
        policy: HipcheckPolicy {
            id: NIGHT_VISION_POLICY_ID.to_owned(),
            version: None,
            source: context.policy_source.clone(),
            recommendation,
        },
        checks: checks.into_iter().collect::<Result<_, _>>()?,
    })
}

fn validate_context(context: &HipcheckReportContext) -> Result<(), HipcheckReportError> {
    if !has_source_repository_url(&context.source_repository_url) {
        return Err(HipcheckReportError::contract(
            "assessment source repository URL must be an absolute HTTP(S) URL",
        ));
    }
    if !has_npm_package_version(&context.target_purl) {
        return Err(HipcheckReportError::contract(
            "assessment target PURL must be a versioned npm package URL",
        ));
    }
    if context.policy_source.trim().is_empty() {
        return Err(HipcheckReportError::contract(
            "assessment policy source must not be empty",
        ));
    }
    Ok(())
}

fn normalize_check(
    native: NativeCheck,
    state: HipcheckCheckState,
    effect: HipcheckEffect,
) -> Result<HipcheckCheck, HipcheckReportError> {
    let plugin = native.plugin_identity()?;
    let summary = native.message.clone();
    let value = serde_json::to_value(&native).expect("native Hipcheck result serializes");
    let error = matches!(state, HipcheckCheckState::Errored).then(|| HipcheckError {
        kind: "plugin".to_owned(),
        message: summary.clone(),
        retryable: false,
    });
    Ok(HipcheckCheck {
        plugin,
        policy: HipcheckCheckPolicy {
            expression: native.policy_expr,
        },
        state,
        effect,
        severity: None,
        summary,
        value,
        concerns: vec![HipcheckConcern {
            kind: "missing-data".to_owned(),
            message: "Hipcheck 3.15 did not report build commit, plugin version, or plugin query."
                .to_owned(),
            details: Some(Map::from_iter([(
                "fields".to_owned(),
                Value::Array(
                    ["hipcheck.commit", "plugin.version", "plugin.query"]
                        .into_iter()
                        .map(|field| Value::String(field.to_owned()))
                        .collect(),
                ),
            )])),
        }],
        started_at: None,
        ended_at: None,
        error,
    })
}

fn has_source_repository_url(source_repository_url: &str) -> bool {
    Url::parse(source_repository_url)
        .is_ok_and(|url| matches!(url.scheme(), "http" | "https") && url.has_host())
}

fn has_npm_package_version(purl: &str) -> bool {
    let Some(package_and_version) = purl.strip_prefix("pkg:npm/") else {
        return false;
    };
    let package_and_version = package_and_version
        .split_once(['?', '#'])
        .map_or(package_and_version, |(path, _)| path);
    let Some((package, version)) = package_and_version.split_once('@') else {
        return false;
    };
    !version.is_empty()
        && !version.contains('@')
        && has_valid_percent_encoding(package)
        && has_valid_percent_encoding(version)
        && is_npm_package_name(package)
}

fn is_npm_package_name(package: &str) -> bool {
    if let Some(scoped_package) = package.strip_prefix("%40") {
        let Some((scope, name)) = scoped_package.split_once('/') else {
            return false;
        };
        !scope.is_empty() && !name.is_empty() && !name.contains('/')
    } else {
        !package.is_empty() && !package.contains('/')
    }
}

fn has_valid_percent_encoding(value: &str) -> bool {
    let mut characters = value.chars();
    while let Some(character) = characters.next() {
        if character != '%' {
            continue;
        }
        let Some(high) = characters.next() else {
            return false;
        };
        let Some(low) = characters.next() else {
            return false;
        };
        if !high.is_ascii_hexdigit() || !low.is_ascii_hexdigit() {
            return false;
        }
    }
    true
}

#[derive(Debug, Deserialize)]
struct NativeHipcheckReport {
    hipcheck_version: String,
    #[serde(default)]
    passing: Vec<NativeCheck>,
    #[serde(default)]
    failing: Vec<NativeCheck>,
    #[serde(default)]
    errored: Vec<NativeCheck>,
    recommendation: NativeRecommendation,
}

#[derive(Debug, Deserialize)]
struct NativeRecommendation {
    kind: NativeRecommendationKind,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
enum NativeRecommendationKind {
    Pass,
    Investigate,
}

#[derive(Debug, Deserialize, Serialize)]
struct NativeCheck {
    #[serde(default)]
    analysis: Option<String>,
    name: String,
    #[serde(default)]
    passed: Option<bool>,
    policy_expr: String,
    #[serde(default)]
    final_value: Option<Value>,
    message: String,
    #[serde(flatten)]
    additional: Map<String, Value>,
}

impl NativeCheck {
    fn plugin_identity(&self) -> Result<HipcheckPluginIdentity, HipcheckReportError> {
        let Some((publisher, name)) = self.name.split_once('/') else {
            return Err(HipcheckReportError::contract(
                "native check name must be publisher/name",
            ));
        };
        if publisher.is_empty() || name.is_empty() || name.contains('/') {
            return Err(HipcheckReportError::contract(
                "native check name must be publisher/name",
            ));
        }
        Ok(HipcheckPluginIdentity {
            name: name.to_owned(),
            publisher: publisher.to_owned(),
            version: UNKNOWN.to_owned(),
            query: UNKNOWN.to_owned(),
        })
    }
}
