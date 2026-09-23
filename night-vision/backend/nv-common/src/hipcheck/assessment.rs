//! Shared lifecycle for persisted Hipcheck assessments.
use super::{
    HipcheckCheckRequest, HipcheckExecutionError, HipcheckExecutionOutput, HipcheckReport,
    HipcheckReportContext, HipcheckRunnerConfig, parse_hipcheck_report, run_hipcheck_check,
    storage::{
        HipcheckExecutionDiagnostics, complete_hipcheck_run, create_queued_hipcheck_run,
        fail_hipcheck_run, mark_hipcheck_run_running,
    },
};
use crate::db::entities::package_versions;
use async_trait::async_trait;
use sea_orm::{ColumnTrait as _, DatabaseConnection, EntityTrait as _, QueryFilter as _};
use std::ffi::OsString;
use uuid::Uuid;

/// Stable, caller-safe behavior for the failure mapping described by RFD 0002
/// and `docs/backend/hipcheck-integration.md`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct HipcheckFailureBehavior {
    kind: &'static str,
    message: &'static str,
    retryable: bool,
}

impl HipcheckFailureBehavior {
    const fn target_resolution() -> Self {
        Self {
            kind: "target-resolution",
            message: "package version has no usable source repository",
            retryable: false,
        }
    }

    const fn nonzero_exit() -> Self {
        Self {
            kind: "nonzero-exit",
            message: "assessment analysis exited unsuccessfully",
            retryable: false,
        }
    }

    const fn report_parse() -> Self {
        Self {
            kind: "report-parse",
            message: "assessment analysis returned an invalid report",
            retryable: false,
        }
    }

    fn execution(error: &HipcheckExecutionError) -> Self {
        let (kind, message) = match error {
            HipcheckExecutionError::Start(_) => {
                ("runner-start", "assessment analysis could not start")
            }
            HipcheckExecutionError::TimedOut { .. } => ("timeout", "assessment analysis timed out"),
            HipcheckExecutionError::OutputLimitExceeded { .. } => (
                "output-limit",
                "assessment analysis exceeded an output limit",
            ),
            HipcheckExecutionError::Read { .. }
            | HipcheckExecutionError::Wait(_)
            | HipcheckExecutionError::ReaderEnded { .. } => {
                ("runner", "assessment analysis process failed")
            }
        };
        Self {
            kind,
            message,
            retryable: error.retryable(),
        }
    }
}

/// Result of resolving a persisted target and creating its durable queue record.
#[derive(Clone, Debug)]
pub struct QueuedAssessment {
    pub id: Uuid,
    pub run_id: i32,
    pub package_version_id: i32,
    pub purl: String,
}

/// Injectable process boundary used by assessment lifecycle tests.
#[async_trait]
pub trait HipcheckExecutor: Send + Sync {
    async fn execute(
        &self,
        config: &HipcheckRunnerConfig,
        request: &HipcheckCheckRequest,
    ) -> Result<HipcheckExecutionOutput, HipcheckExecutionError>;
}

/// The production executor invokes the hermetic runner.
pub struct RunnerExecutor;

#[async_trait]
impl HipcheckExecutor for RunnerExecutor {
    async fn execute(
        &self,
        config: &HipcheckRunnerConfig,
        request: &HipcheckCheckRequest,
    ) -> Result<HipcheckExecutionOutput, HipcheckExecutionError> {
        run_hipcheck_check(config, request).await
    }
}

/// Create a queued assessment for an already elaborated package version.
pub async fn queue_assessment(
    db: &DatabaseConnection,
    affected_purl: &str,
    target_purl: &str,
) -> Result<QueuedAssessment, AssessmentError> {
    queue_assessment_with_id(db, Uuid::now_v7(), affected_purl, target_purl).await
}

/// Create a queued assessment using an ID owned by a higher-level workflow.
///
/// Upgrade assessments use their durable assessment ID here so that their
/// user-facing report can load the normalized Hipcheck evidence for the same
/// assessment without a second correlation table.
pub async fn queue_assessment_with_id(
    db: &DatabaseConnection,
    id: Uuid,
    affected_purl: &str,
    target_purl: &str,
) -> Result<QueuedAssessment, AssessmentError> {
    let version = package_versions::Entity::find()
        .filter(package_versions::Column::PackageUrl.eq(target_purl))
        .one(db)
        .await
        .map_err(AssessmentError::Database)?
        .ok_or_else(|| AssessmentError::UnknownPurl(target_purl.to_owned()))?;
    let run_id = create_queued_hipcheck_run(db, version.id, affected_purl, &id)
        .await
        .map_err(AssessmentError::Database)?;
    Ok(QueuedAssessment {
        id,
        run_id,
        package_version_id: version.id,
        purl: version.package_url,
    })
}

/// Execute a queued assessment, persisting every terminal outcome.
pub async fn execute_queued_assessment(
    db: &DatabaseConnection,
    queued: &QueuedAssessment,
    config: &HipcheckRunnerConfig,
) -> Result<(), AssessmentError> {
    execute_queued_assessment_with_executor(db, queued, config, &RunnerExecutor).await
}

/// Execute a queued assessment with an injected process boundary.
pub async fn execute_queued_assessment_with_executor<E: HipcheckExecutor>(
    db: &DatabaseConnection,
    queued: &QueuedAssessment,
    config: &HipcheckRunnerConfig,
    executor: &E,
) -> Result<(), AssessmentError> {
    let running_transition_applied = mark_hipcheck_run_running(db, queued.run_id)
        .await
        .map_err(AssessmentError::Database)?;
    if !running_transition_applied {
        // Startup reconciliation or another terminal writer already claimed this
        // run. Do not resurrect it or replace its durable outcome.
        return Ok(());
    }
    let version = package_versions::Entity::find_by_id(queued.package_version_id)
        .one(db)
        .await
        .map_err(AssessmentError::Database)?
        .ok_or_else(|| AssessmentError::UnknownPurl(queued.purl.clone()))?;
    let Some(target) = version
        .source_repository
        .filter(|value| value.starts_with("http://") || value.starts_with("https://"))
    else {
        let failure = HipcheckFailureBehavior::target_resolution();
        return persist_failure(
            db,
            queued.run_id,
            failure_diagnostics(failure, None, b"", b""),
            None,
        )
        .await;
    };
    let report_context = HipcheckReportContext {
        target_purl: queued.purl.clone(),
        source_repository_url: target.clone(),
        policy_source: config.policy_path.to_string(),
    };
    match executor
        .execute(
            config,
            &HipcheckCheckRequest {
                arguments: vec![OsString::from(target)],
            },
        )
        .await
    {
        Ok(output) => {
            if output.status.success() || has_report_candidate(output.json.as_deref()) {
                // Hipcheck may use a nonzero exit for an INVESTIGATE result or
                // a report containing a plugin error. Preserve every complete,
                // structured check in that report.
                complete_output(db, queued.run_id, output, &report_context).await
            } else {
                let failure = HipcheckFailureBehavior::nonzero_exit();
                persist_failure(
                    db,
                    queued.run_id,
                    failure_diagnostics(
                        failure,
                        output.status.code(),
                        &output.stdout,
                        &output.stderr,
                    ),
                    None,
                )
                .await
            }
        }
        Err(error) => {
            let (stdout, stderr) = error_diagnostics(&error);
            let failure = HipcheckFailureBehavior::execution(&error);
            persist_failure(
                db,
                queued.run_id,
                failure_diagnostics(failure, None, &stdout, &stderr),
                None,
            )
            .await
        }
    }
}

async fn complete_output(
    db: &DatabaseConnection,
    id: i32,
    output: HipcheckExecutionOutput,
    context: &HipcheckReportContext,
) -> Result<(), AssessmentError> {
    let raw =
        String::from_utf8_lossy(output.json.as_deref().unwrap_or(&output.stdout)).into_owned();
    let report = match normalize_report(&raw, context) {
        Ok(report) => report,
        Err(failure) => {
            return persist_failure(
                db,
                id,
                failure_diagnostics(
                    failure,
                    output.status.code(),
                    &output.stdout,
                    &output.stderr,
                ),
                Some(&raw),
            )
            .await;
        }
    };
    let result = complete_hipcheck_run(
        db,
        id,
        &raw,
        &report,
        &diagnostics(
            "completed",
            "",
            output.status.code(),
            false,
            &output.stdout,
            &output.stderr,
        ),
    )
    .await;
    require_terminal_persistence_applied(result, id)
}

fn has_report_candidate(json: Option<&[u8]>) -> bool {
    json.is_some_and(|json| json.iter().any(|byte| !byte.is_ascii_whitespace()))
}

fn normalize_report(
    raw: &str,
    context: &HipcheckReportContext,
) -> Result<HipcheckReport, HipcheckFailureBehavior> {
    parse_hipcheck_report(raw, context).map_err(|_| HipcheckFailureBehavior::report_parse())
}

async fn persist_failure(
    db: &DatabaseConnection,
    id: i32,
    diagnostics: HipcheckExecutionDiagnostics,
    raw: Option<&str>,
) -> Result<(), AssessmentError> {
    let result = fail_hipcheck_run(db, id, &diagnostics, raw).await;
    require_terminal_persistence_applied(result, id)
}

fn require_terminal_persistence_applied(
    result: Result<bool, sea_orm::DbErr>,
    run_id: i32,
) -> Result<(), AssessmentError> {
    match result {
        Ok(true) => Ok(()),
        Ok(false) => Err(AssessmentError::TerminalOutcomeNotPersisted(run_id)),
        Err(error) => Err(AssessmentError::Database(error)),
    }
}

fn diagnostics(
    kind: &str,
    message: &str,
    exit_status: Option<i32>,
    retryable: bool,
    stdout: &[u8],
    stderr: &[u8],
) -> HipcheckExecutionDiagnostics {
    HipcheckExecutionDiagnostics {
        status: "failed".to_owned(),
        stdout: String::from_utf8_lossy(stdout).into_owned(),
        stderr: String::from_utf8_lossy(stderr).into_owned(),
        exit_status,
        error_kind: (kind != "completed").then(|| kind.to_owned()),
        error_message: (!message.is_empty()).then(|| message.to_owned()),
        retryable: Some(retryable),
    }
}

fn failure_diagnostics(
    failure: HipcheckFailureBehavior,
    exit_status: Option<i32>,
    stdout: &[u8],
    stderr: &[u8],
) -> HipcheckExecutionDiagnostics {
    diagnostics(
        failure.kind,
        failure.message,
        exit_status,
        failure.retryable,
        stdout,
        stderr,
    )
}

fn error_diagnostics(error: &HipcheckExecutionError) -> (Vec<u8>, Vec<u8>) {
    match error {
        HipcheckExecutionError::TimedOut { stdout, stderr, .. }
        | HipcheckExecutionError::OutputLimitExceeded { stdout, stderr, .. } => {
            (stdout.clone(), stderr.clone())
        }
        _ => (Vec::new(), Vec::new()),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AssessmentError {
    #[error("unknown persisted package version: {0}")]
    UnknownPurl(String),
    #[error("assessment database operation failed")]
    Database(#[source] sea_orm::DbErr),
    #[error("failed to persist terminal assessment outcome for run {0}")]
    TerminalOutcomeNotPersisted(i32),
}

#[cfg(test)]
mod tests {
    use super::{
        AssessmentError, HipcheckFailureBehavior, has_report_candidate, normalize_report,
        require_terminal_persistence_applied,
    };
    use crate::hipcheck::{HipcheckExecutionError, HipcheckOutputStream, HipcheckReportContext};
    use sea_orm::DbErr;
    use std::time::Duration;

    #[test]
    fn execution_failure_diagnostic_does_not_include_source_error_text() {
        let error = HipcheckExecutionError::Start(std::io::Error::other(
            "token=correct-horse-battery-staple; external tool stderr",
        ));

        let behavior = HipcheckFailureBehavior::execution(&error);
        let diagnostic = behavior.message;

        assert_eq!(behavior.kind, "runner-start");
        assert!(!behavior.retryable);
        assert_eq!(diagnostic, "assessment analysis could not start");
        assert!(!diagnostic.contains("correct-horse-battery-staple"));
        assert!(!diagnostic.contains("external tool stderr"));
    }

    #[test]
    fn execution_failures_have_stable_retryability() {
        let cases = [
            (
                HipcheckExecutionError::TimedOut {
                    timeout: Duration::from_secs(1),
                    stdout: b"partial JSON".to_vec(),
                    stderr: Vec::new(),
                },
                "timeout",
                true,
            ),
            (
                HipcheckExecutionError::OutputLimitExceeded {
                    stream: HipcheckOutputStream::Json,
                    limit: 10,
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                },
                "output-limit",
                false,
            ),
            (
                HipcheckExecutionError::Read {
                    stream: HipcheckOutputStream::Stdout,
                    source: std::io::Error::other("transient read"),
                },
                "runner",
                true,
            ),
            (
                HipcheckExecutionError::Wait(std::io::Error::other("transient wait")),
                "runner",
                true,
            ),
            (
                HipcheckExecutionError::ReaderEnded {
                    stream: HipcheckOutputStream::Stderr,
                },
                "runner",
                true,
            ),
        ];

        for (error, expected_kind, expected_retryable) in cases {
            let behavior = HipcheckFailureBehavior::execution(&error);
            assert_eq!(behavior.kind, expected_kind);
            assert_eq!(behavior.retryable, expected_retryable);
        }
    }

    #[test]
    fn nonzero_exit_with_json_is_treated_as_a_report_candidate() {
        assert!(has_report_candidate(Some(
            br#"{"recommendation":"INVESTIGATE"}"#
        )));
        assert!(!has_report_candidate(Some(b" \n\t")));
        assert!(!has_report_candidate(None));
    }

    #[test]
    fn malformed_or_incomplete_json_maps_to_one_safe_integration_error() {
        let context = HipcheckReportContext {
            target_purl: "pkg:npm/example@1.0.0".to_owned(),
            source_repository_url: "https://github.com/example/project".to_owned(),
            policy_source: "/opt/night-vision/Hipcheck.kdl".to_owned(),
        };

        for raw in ["{", r#"{"display":"PASS: ignore this text"}"#] {
            let failure = normalize_report(raw, &context)
                .expect_err("invalid reports must not produce normalized findings");
            assert_eq!(failure, HipcheckFailureBehavior::report_parse());
            assert!(!failure.message.contains("display"));
            assert!(!failure.message.contains("PASS"));
        }
    }

    #[test]
    fn target_and_empty_nonzero_failures_are_non_retryable_missing_evidence() {
        assert_eq!(
            HipcheckFailureBehavior::target_resolution().kind,
            "target-resolution"
        );
        assert!(!HipcheckFailureBehavior::target_resolution().retryable);
        assert_eq!(HipcheckFailureBehavior::nonzero_exit().kind, "nonzero-exit");
        assert!(!HipcheckFailureBehavior::nonzero_exit().retryable);
    }

    #[test]
    fn terminal_persistence_requires_an_applied_transition() {
        let error = require_terminal_persistence_applied(Ok(false), 42).unwrap_err();

        assert!(matches!(
            error,
            AssessmentError::TerminalOutcomeNotPersisted(42)
        ));
    }

    #[test]
    fn terminal_persistence_preserves_database_errors() {
        let error = require_terminal_persistence_applied(
            Err(DbErr::Custom("database unavailable".to_owned())),
            42,
        )
        .unwrap_err();

        assert!(
            matches!(error, AssessmentError::Database(DbErr::Custom(message)) if message == "database unavailable")
        );
    }
}
