//! Transactional persistence for untrusted Hipcheck evidence.
use super::{HipcheckCheck, HipcheckReport};
use crate::db::entities::{hipcheck_checks, hipcheck_concerns, hipcheck_findings, hipcheck_runs};
use sea_orm::{
    ActiveModelTrait as _, ActiveValue::Set, ColumnTrait as _, ConnectionTrait, DbErr,
    EntityTrait as _, QueryFilter as _, QueryOrder as _, QuerySelect as _, TransactionSession as _,
    TransactionTrait,
};
use serde_json::json;

/// Maximum bytes retained for each process diagnostic or raw report payload.
pub const MAX_STORED_HIPCHECK_OUTPUT_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HipcheckExecutionDiagnostics {
    pub status: String,
    pub stdout: String,
    pub stderr: String,
    pub exit_status: Option<i32>,
    pub error_kind: Option<String>,
    pub error_message: Option<String>,
    pub retryable: Option<bool>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredHipcheckRun {
    pub run: hipcheck_runs::Model,
    pub checks: Vec<hipcheck_checks::Model>,
    pub concerns: Vec<hipcheck_concerns::Model>,
    pub findings: Vec<hipcheck_findings::Model>,
}

/// Create the durable record before an assessment is dispatched.
pub async fn create_queued_hipcheck_run<C: ConnectionTrait>(
    db: &C,
    package_version_id: i32,
) -> Result<i32, DbErr> {
    hipcheck_runs::ActiveModel {
        package_version_id: Set(package_version_id),
        status: Set("queued".to_owned()),
        raw_json: Set(None),
        raw_json_bytes: Set(0),
        raw_json_truncated: Set(false),
        stdout: Set(None),
        stdout_truncated: Set(false),
        stderr: Set(None),
        stderr_truncated: Set(false),
        exit_status: Set(None),
        error_kind: Set(None),
        error_message: Set(None),
        retryable: Set(None),
        schema_version: Set(None),
        hipcheck_version: Set(None),
        hipcheck_commit: Set(None),
        target_kind: Set(None),
        target_purl: Set(None),
        source_repository_url: Set(None),
        policy_id: Set(None),
        policy_version: Set(None),
        policy_source: Set(None),
        policy_recommendation: Set(None),
        ..Default::default()
    }
    .insert(db)
    .await
    .map(|run| run.id)
}

/// Mark a previously queued assessment as executing.
pub async fn mark_hipcheck_run_running<C: ConnectionTrait>(
    db: &C,
    run_id: i32,
) -> Result<(), DbErr> {
    hipcheck_runs::ActiveModel {
        id: Set(run_id),
        status: Set("running".to_owned()),
        ..Default::default()
    }
    .update(db)
    .await
    .map(|_| ())
}

/// Record a terminal failure while preserving bounded diagnostics.
pub async fn fail_hipcheck_run<C: ConnectionTrait>(
    db: &C,
    run_id: i32,
    diagnostics: &HipcheckExecutionDiagnostics,
    raw_json: Option<&str>,
) -> Result<(), DbErr> {
    let (stdout, stdout_truncated) = bound(&diagnostics.stdout);
    let (stderr, stderr_truncated) = bound(&diagnostics.stderr);
    let (raw_json, raw_json_truncated, raw_json_bytes) =
        raw_json.map_or((None, false, 0), |value| {
            let (bounded, truncated) = bound(value);
            (
                Some(bounded),
                truncated,
                i32::try_from(value.len()).unwrap_or(i32::MAX),
            )
        });
    hipcheck_runs::ActiveModel {
        id: Set(run_id),
        status: Set("failed".to_owned()),
        raw_json: Set(raw_json),
        raw_json_bytes: Set(raw_json_bytes),
        raw_json_truncated: Set(raw_json_truncated),
        stdout: Set(Some(stdout)),
        stdout_truncated: Set(stdout_truncated),
        stderr: Set(Some(stderr)),
        stderr_truncated: Set(stderr_truncated),
        exit_status: Set(diagnostics.exit_status),
        error_kind: Set(diagnostics.error_kind.clone()),
        error_message: Set(diagnostics
            .error_message
            .as_deref()
            .map(|value| bound(value).0)),
        retryable: Set(diagnostics.retryable),
        ..Default::default()
    }
    .update(db)
    .await
    .map(|_| ())
}

/// Complete an existing run and add normalized evidence atomically.
pub async fn complete_hipcheck_run<C: ConnectionTrait + TransactionTrait>(
    db: &C,
    run_id: i32,
    raw_json: &str,
    report: &HipcheckReport,
    diagnostics: &HipcheckExecutionDiagnostics,
) -> Result<(), DbErr> {
    let transaction = db.begin().await?;
    let raw_json_bytes = i32::try_from(raw_json.len()).unwrap_or(i32::MAX);
    let (raw_json, raw_json_truncated) = bound(raw_json);
    let (stdout, stdout_truncated) = bound(&diagnostics.stdout);
    let (stderr, stderr_truncated) = bound(&diagnostics.stderr);
    hipcheck_runs::ActiveModel {
        id: Set(run_id),
        status: Set("completed".to_owned()),
        raw_json: Set(Some(raw_json)),
        raw_json_bytes: Set(raw_json_bytes),
        raw_json_truncated: Set(raw_json_truncated),
        stdout: Set(Some(stdout)),
        stdout_truncated: Set(stdout_truncated),
        stderr: Set(Some(stderr)),
        stderr_truncated: Set(stderr_truncated),
        exit_status: Set(diagnostics.exit_status),
        error_kind: Set(None),
        error_message: Set(None),
        retryable: Set(Some(false)),
        schema_version: Set(Some(report.schema_version.clone())),
        hipcheck_version: Set(Some(report.hipcheck.version.clone())),
        hipcheck_commit: Set(Some(report.hipcheck.commit.clone())),
        target_kind: Set(Some(report.target.kind.clone())),
        target_purl: Set(report.target.purl.clone()),
        source_repository_url: Set(Some(report.target.source_repository_url.clone())),
        policy_id: Set(Some(report.policy.id.clone())),
        policy_version: Set(report.policy.version.clone()),
        policy_source: Set(Some(report.policy.source.clone())),
        policy_recommendation: Set(Some(recommendation(report))),
        ..Default::default()
    }
    .update(&transaction)
    .await?;
    for (ordinal, check) in report.checks.iter().enumerate() {
        store_check(&transaction, run_id, ordinal, check).await?;
    }
    transaction.commit().await
}

/// List the newest persisted assessments for one resolved package version.
pub async fn list_hipcheck_runs<C: ConnectionTrait>(
    db: &C,
    package_version_id: i32,
    limit: u64,
) -> Result<Vec<hipcheck_runs::Model>, DbErr> {
    hipcheck_runs::Entity::find()
        .filter(hipcheck_runs::Column::PackageVersionId.eq(package_version_id))
        .order_by_desc(hipcheck_runs::Column::CreatedAt)
        .limit(limit)
        .all(db)
        .await
}

/// Store a parsed report and bounded, explicitly selected execution diagnostics.
///
/// The caller must not place command environments, arguments containing credentials, or
/// unredacted secrets in `diagnostics`; this boundary never reads those values itself.
pub async fn store_hipcheck_run<C: ConnectionTrait + TransactionTrait>(
    db: &C,
    package_version_id: i32,
    raw_json: &str,
    report: &HipcheckReport,
    diagnostics: &HipcheckExecutionDiagnostics,
) -> Result<i32, DbErr> {
    let transaction = db.begin().await?;
    let raw_json_bytes = i32::try_from(raw_json.len()).unwrap_or(i32::MAX);
    let (raw_json, raw_json_truncated) = bound(raw_json);
    let (stdout, stdout_truncated) = bound(&diagnostics.stdout);
    let (stderr, stderr_truncated) = bound(&diagnostics.stderr);
    let error_message = diagnostics
        .error_message
        .as_deref()
        .map(|value| bound(value).0);
    let run = hipcheck_runs::ActiveModel {
        id: Default::default(),
        package_version_id: Set(package_version_id),
        status: Set(diagnostics.status.clone()),
        raw_json: Set(Some(raw_json)),
        raw_json_bytes: Set(raw_json_bytes),
        raw_json_truncated: Set(raw_json_truncated),
        stdout: Set(Some(stdout)),
        stdout_truncated: Set(stdout_truncated),
        stderr: Set(Some(stderr)),
        stderr_truncated: Set(stderr_truncated),
        exit_status: Set(diagnostics.exit_status),
        error_kind: Set(diagnostics.error_kind.clone()),
        error_message: Set(error_message),
        retryable: Set(diagnostics.retryable),
        schema_version: Set(Some(report.schema_version.clone())),
        hipcheck_version: Set(Some(report.hipcheck.version.clone())),
        hipcheck_commit: Set(Some(report.hipcheck.commit.clone())),
        target_kind: Set(Some(report.target.kind.clone())),
        target_purl: Set(report.target.purl.clone()),
        source_repository_url: Set(Some(report.target.source_repository_url.clone())),
        policy_id: Set(Some(report.policy.id.clone())),
        policy_version: Set(report.policy.version.clone()),
        policy_source: Set(Some(report.policy.source.clone())),
        policy_recommendation: Set(Some(recommendation(report))),
        created_at: Default::default(),
    }
    .insert(&transaction)
    .await?;
    for (ordinal, check) in report.checks.iter().enumerate() {
        store_check(&transaction, run.id, ordinal, check).await?;
    }
    transaction.commit().await?;
    Ok(run.id)
}

/// Load the normalized records and original bounded evidence for a stored run.
pub async fn load_hipcheck_run<C: ConnectionTrait>(
    db: &C,
    run_id: i32,
) -> Result<Option<StoredHipcheckRun>, DbErr> {
    let Some(run) = hipcheck_runs::Entity::find_by_id(run_id).one(db).await? else {
        return Ok(None);
    };
    let checks = hipcheck_checks::Entity::find()
        .filter(hipcheck_checks::Column::RunId.eq(run_id))
        .order_by_asc(hipcheck_checks::Column::Ordinal)
        .all(db)
        .await?;
    let check_ids: Vec<i32> = checks.iter().map(|check| check.id).collect();
    let concerns = hipcheck_concerns::Entity::find()
        .filter(hipcheck_concerns::Column::CheckId.is_in(check_ids.clone()))
        .order_by_asc(hipcheck_concerns::Column::Ordinal)
        .all(db)
        .await?;
    let findings = hipcheck_findings::Entity::find()
        .filter(hipcheck_findings::Column::RunId.eq(run_id))
        .order_by_asc(hipcheck_findings::Column::Ordinal)
        .all(db)
        .await?;
    Ok(Some(StoredHipcheckRun {
        run,
        checks,
        concerns,
        findings,
    }))
}

async fn store_check<C: ConnectionTrait>(
    db: &C,
    run_id: i32,
    ordinal: usize,
    check: &HipcheckCheck,
) -> Result<(), DbErr> {
    let ordinal = i32::try_from(ordinal).unwrap_or(i32::MAX);
    let stored = hipcheck_checks::ActiveModel {
        id: Default::default(),
        run_id: Set(run_id),
        ordinal: Set(ordinal),
        plugin_name: Set(check.plugin.name.clone()),
        plugin_publisher: Set(check.plugin.publisher.clone()),
        plugin_version: Set(check.plugin.version.clone()),
        plugin_query: Set(check.plugin.query.clone()),
        policy_expression: Set(check.policy.expression.clone()),
        state: Set(state(check).to_owned()),
        effect: Set(effect(check)),
        severity: Set(check.severity.clone()),
        summary: Set(bound(&check.summary).0),
        value: Set(check.value.clone()),
        started_at: Set(check.started_at.clone()),
        ended_at: Set(check.ended_at.clone()),
        error_kind: Set(check.error.as_ref().map(|error| error.kind.clone())),
        error_message: Set(check.error.as_ref().map(|error| bound(&error.message).0)),
        error_retryable: Set(check.error.as_ref().map(|error| error.retryable)),
    }
    .insert(db)
    .await?;
    for (concern_ordinal, concern) in check.concerns.iter().enumerate() {
        hipcheck_concerns::ActiveModel {
            id: Default::default(),
            check_id: Set(stored.id),
            ordinal: Set(i32::try_from(concern_ordinal).unwrap_or(i32::MAX)),
            kind: Set(concern.kind.clone()),
            message: Set(bound(&concern.message).0),
            details: Set(concern.details.clone().map(serde_json::Value::Object)),
        }
        .insert(db)
        .await?;
    }
    let finding_kind = if matches!(check.state, super::HipcheckCheckState::Unsupported) {
        "missing-check"
    } else if matches!(check.state, super::HipcheckCheckState::Errored) {
        "check-error"
    } else {
        "check-result"
    };
    hipcheck_findings::ActiveModel { id: Default::default(), run_id: Set(run_id), check_id: Set(Some(stored.id)), ordinal: Set(ordinal), kind: Set(finding_kind.to_owned()), effect: Set(effect(check)), severity: Set(check.severity.clone()), summary: Set(bound(&check.summary).0), evidence: Set(json!({"plugin": {"publisher": check.plugin.publisher, "name": check.plugin.name, "version": check.plugin.version, "query": check.plugin.query}, "policy_expression": check.policy.expression, "state": state(check), "value": check.value, "error_kind": check.error.as_ref().map(|error| &error.kind), "error_retryable": check.error.as_ref().map(|error| error.retryable)})) }.insert(db).await?;
    Ok(())
}
fn bound(value: &str) -> (String, bool) {
    if value.len() <= MAX_STORED_HIPCHECK_OUTPUT_BYTES {
        return (value.to_owned(), false);
    }
    let mut end = MAX_STORED_HIPCHECK_OUTPUT_BYTES;
    while !value.is_char_boundary(end) {
        end = end
            .checked_sub(1)
            .expect("UTF-8 boundary search starts from a nonzero byte index");
    }
    (value[..end].to_owned(), true)
}
fn effect(check: &HipcheckCheck) -> String {
    match check.effect {
        super::HipcheckEffect::Blocking => "blocking",
        super::HipcheckEffect::Review => "review",
        super::HipcheckEffect::Context => "context",
        super::HipcheckEffect::MissingCheck => "missing-check",
    }
    .to_owned()
}
fn state(check: &HipcheckCheck) -> &'static str {
    match check.state {
        super::HipcheckCheckState::Passed => "passed",
        super::HipcheckCheckState::Failed => "failed",
        super::HipcheckCheckState::Skipped => "skipped",
        super::HipcheckCheckState::Unsupported => "unsupported",
        super::HipcheckCheckState::Errored => "errored",
    }
}
fn recommendation(report: &HipcheckReport) -> String {
    format!("{:?}", report.policy.recommendation).to_uppercase()
}

#[cfg(test)]
mod tests {
    use super::{MAX_STORED_HIPCHECK_OUTPUT_BYTES, bound};

    #[test]
    fn output_bound_preserves_utf8_and_reports_truncation() {
        let input = format!("{}é", "x".repeat(MAX_STORED_HIPCHECK_OUTPUT_BYTES));
        let (stored, truncated) = bound(&input);
        assert!(truncated);
        assert_eq!(stored.len(), MAX_STORED_HIPCHECK_OUTPUT_BYTES);
        assert!(stored.is_char_boundary(stored.len()));
    }
}
