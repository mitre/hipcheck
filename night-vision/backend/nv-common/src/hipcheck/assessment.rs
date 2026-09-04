//! Shared lifecycle for persisted Hipcheck assessments.
use super::{
    HipcheckCheckRequest, HipcheckExecutionError, HipcheckExecutionOutput, HipcheckRunnerConfig,
    parse_hipcheck_report, run_hipcheck_check,
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
    let version = package_versions::Entity::find()
        .filter(package_versions::Column::PackageUrl.eq(target_purl))
        .one(db)
        .await
        .map_err(AssessmentError::Database)?
        .ok_or_else(|| AssessmentError::UnknownPurl(target_purl.to_owned()))?;
    let id = Uuid::now_v7();
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
    mark_hipcheck_run_running(db, queued.run_id)
        .await
        .map_err(AssessmentError::Database)?;
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
    match executor
        .execute(
            config,
            &HipcheckCheckRequest {
                arguments: vec![OsString::from(target)],
            },
        )
        .await
    {
        Ok(output) if output.status.success() => complete_output(db, queued.run_id, output).await,
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
                    &error.to_string(),
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
) -> Result<(), AssessmentError> {
    let raw =
        String::from_utf8_lossy(output.json.as_deref().unwrap_or(&output.stdout)).into_owned();
    let report = match parse_hipcheck_report(&raw) {
        Ok(report) => report,
        Err(error) => {
            return persist_failure(
                db,
                id,
                diagnostics(
                    "report-parse",
                    &error.to_string(),
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
    complete_hipcheck_run(
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
    .await
    .map_err(AssessmentError::Database)
}

async fn persist_failure(
    db: &DatabaseConnection,
    id: i32,
    diagnostics: HipcheckExecutionDiagnostics,
    raw: Option<&str>,
) -> Result<(), AssessmentError> {
    fail_hipcheck_run(db, id, &diagnostics, raw)
        .await
        .map_err(AssessmentError::Database)
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

#[derive(Debug, thiserror::Error)]
pub enum AssessmentError {
    #[error("unknown persisted package version: {0}")]
    UnknownPurl(String),
    #[error("assessment database operation failed")]
    Database(#[source] sea_orm::DbErr),
}
