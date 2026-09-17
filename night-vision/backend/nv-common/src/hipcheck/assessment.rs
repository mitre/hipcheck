//! Shared lifecycle for persisted Hipcheck assessments.
use super::{
    HipcheckCheckRequest, HipcheckExecutionError, HipcheckExecutionOutput, HipcheckReportContext,
    HipcheckRunnerConfig, parse_hipcheck_report, run_hipcheck_check,
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
        return persist_failure(
            db,
            queued.run_id,
            diagnostics(
                "target-resolution",
                "package version has no usable source repository",
                None,
                false,
                b"",
                b"",
            ),
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
        Ok(output) if output.status.success() => {
            complete_output(db, queued.run_id, output, &report_context).await
        }
        Ok(output) => {
            persist_failure(
                db,
                queued.run_id,
                diagnostics(
                    "nonzero-exit",
                    "Hipcheck exited unsuccessfully",
                    output.status.code(),
                    false,
                    &output.stdout,
                    &output.stderr,
                ),
                None,
            )
            .await
        }
        Err(error) => {
            let (stdout, stderr) = error_diagnostics(&error);
            persist_failure(
                db,
                queued.run_id,
                diagnostics(
                    error_kind(&error),
                    safe_error_message(&error),
                    None,
                    error.retryable(),
                    &stdout,
                    &stderr,
                ),
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
    let report = match parse_hipcheck_report(&raw, context) {
        Ok(report) => report,
        Err(_) => {
            return persist_failure(
                db,
                id,
                diagnostics(
                    "report-parse",
                    "assessment analysis returned an invalid report",
                    output.status.code(),
                    false,
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
fn error_diagnostics(error: &HipcheckExecutionError) -> (Vec<u8>, Vec<u8>) {
    match error {
        HipcheckExecutionError::TimedOut { stdout, stderr, .. }
        | HipcheckExecutionError::OutputLimitExceeded { stdout, stderr, .. } => {
            (stdout.clone(), stderr.clone())
        }
        _ => (Vec::new(), Vec::new()),
    }
}
fn error_kind(error: &HipcheckExecutionError) -> &'static str {
    match error {
        HipcheckExecutionError::Start(_) => "runner-start",
        HipcheckExecutionError::TimedOut { .. } => "timeout",
        HipcheckExecutionError::OutputLimitExceeded { .. } => "output-limit",
        _ => "runner",
    }
}

/// Returns a stable diagnostic for process-boundary failures.
///
/// Do not use `HipcheckExecutionError`'s `Display` output here: its source
/// chain and captured tool output are not safe to persist as a normal
/// assessment error or to reflect through an API.
fn safe_error_message(error: &HipcheckExecutionError) -> &'static str {
    let _ = error;
    "assessment analysis failed"
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
    use super::{AssessmentError, require_terminal_persistence_applied, safe_error_message};
    use crate::hipcheck::HipcheckExecutionError;
    use sea_orm::DbErr;

    #[test]
    fn execution_failure_diagnostic_does_not_include_source_error_text() {
        let error = HipcheckExecutionError::Start(std::io::Error::other(
            "token=correct-horse-battery-staple; external tool stderr",
        ));

        let diagnostic = safe_error_message(&error);

        assert_eq!(diagnostic, "assessment analysis failed");
        assert!(!diagnostic.contains("correct-horse-battery-staple"));
        assert!(!diagnostic.contains("external tool stderr"));
    }

    #[test]
    fn terminal_persistence_requires_an_applied_transition() {
        let error = require_terminal_persistence_applied(Ok(false), 42).unwrap_err();

        assert!(matches!(error, AssessmentError::TerminalOutcomeNotPersisted(42)));
    }

    #[test]
    fn terminal_persistence_preserves_database_errors() {
        let error = require_terminal_persistence_applied(
            Err(DbErr::Custom("database unavailable".to_owned())),
            42,
        )
        .unwrap_err();

        assert!(matches!(error, AssessmentError::Database(DbErr::Custom(message)) if message == "database unavailable"));
    }
}
