//! Transactional persistence for untrusted Hipcheck evidence.
use super::{HipcheckCheck, HipcheckReport};
use crate::db::entities::{
	hipcheck_checks, hipcheck_concerns, hipcheck_findings, hipcheck_runs, package_versions,
	upgrade_assessments,
};
use sea_orm::{
	ActiveModelTrait as _,
	ActiveValue::Set,
	ColumnTrait as _, ConnectionTrait, DbErr, EntityTrait as _, QueryFilter as _, QueryOrder as _,
	QuerySelect as _, TransactionSession as _, TransactionTrait,
	sea_query::{Expr, Query},
};
use serde_json::json;
use uuid::Uuid;

/// Maximum bytes retained for each process diagnostic or raw report payload.
pub const MAX_STORED_HIPCHECK_OUTPUT_BYTES: usize = 64 * 1024;

/// Stable error reported when a server restart interrupts an in-process assessment.
pub const ASSESSMENT_INTERRUPTED_ERROR_KIND: &str = "server-restart";
/// Caller-safe explanation accompanying [`ASSESSMENT_INTERRUPTED_ERROR_KIND`].
pub const ASSESSMENT_INTERRUPTED_ERROR_MESSAGE: &str =
	"assessment was interrupted because the Night Vision server restarted";

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
	affected_purl: &str,
	assessment_id: &Uuid,
) -> Result<i32, DbErr> {
	hipcheck_runs::ActiveModel {
		assessment_id: Set(assessment_id.to_string()),
		package_version_id: Set(package_version_id),
		affected_purl: Set(Some(affected_purl.to_owned())),
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
) -> Result<bool, DbErr> {
	hipcheck_runs::Entity::update_many()
		.col_expr(hipcheck_runs::Column::Status, Expr::value("running"))
		.filter(hipcheck_runs::Column::Id.eq(run_id))
		.filter(hipcheck_runs::Column::Status.eq("queued"))
		.exec(db)
		.await
		.map(|result| result.rows_affected == 1)
}

/// Record a terminal failure while preserving bounded diagnostics.
pub async fn fail_hipcheck_run<C: ConnectionTrait>(
	db: &C,
	run_id: i32,
	diagnostics: &HipcheckExecutionDiagnostics,
	raw_json: Option<&str>,
) -> Result<bool, DbErr> {
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
	hipcheck_runs::Entity::update_many()
		.col_expr(hipcheck_runs::Column::Status, Expr::value("failed"))
		.col_expr(hipcheck_runs::Column::RawJson, Expr::value(raw_json))
		.col_expr(
			hipcheck_runs::Column::RawJsonBytes,
			Expr::value(raw_json_bytes),
		)
		.col_expr(
			hipcheck_runs::Column::RawJsonTruncated,
			Expr::value(raw_json_truncated),
		)
		.col_expr(hipcheck_runs::Column::Stdout, Expr::value(Some(stdout)))
		.col_expr(
			hipcheck_runs::Column::StdoutTruncated,
			Expr::value(stdout_truncated),
		)
		.col_expr(hipcheck_runs::Column::Stderr, Expr::value(Some(stderr)))
		.col_expr(
			hipcheck_runs::Column::StderrTruncated,
			Expr::value(stderr_truncated),
		)
		.col_expr(
			hipcheck_runs::Column::ExitStatus,
			Expr::value(diagnostics.exit_status),
		)
		.col_expr(
			hipcheck_runs::Column::ErrorKind,
			Expr::value(diagnostics.error_kind.clone()),
		)
		.col_expr(
			hipcheck_runs::Column::ErrorMessage,
			Expr::value(
				diagnostics
					.error_message
					.as_deref()
					.map(|value| bound(value).0),
			),
		)
		.col_expr(
			hipcheck_runs::Column::Retryable,
			Expr::value(diagnostics.retryable),
		)
		.filter(hipcheck_runs::Column::Id.eq(run_id))
		.filter(hipcheck_runs::Column::Status.eq("running"))
		.exec(db)
		.await
		.map(|result| result.rows_affected == 1)
}

/// Fail unfinished assessments left behind by a server that exited.
///
/// This is a single compare-and-update operation. It only claims `queued` or
/// `running` rows, so it is idempotent and cannot replace a terminal result.
pub async fn reconcile_abandoned_hipcheck_runs<C: ConnectionTrait>(db: &C) -> Result<u64, DbErr> {
	hipcheck_runs::Entity::update_many()
		.col_expr(hipcheck_runs::Column::Status, Expr::value("failed"))
		.col_expr(hipcheck_runs::Column::RawJson, Expr::value(None::<String>))
		.col_expr(hipcheck_runs::Column::RawJsonBytes, Expr::value(0))
		.col_expr(hipcheck_runs::Column::RawJsonTruncated, Expr::value(false))
		.col_expr(
			hipcheck_runs::Column::Stdout,
			Expr::value(Some(String::new())),
		)
		.col_expr(hipcheck_runs::Column::StdoutTruncated, Expr::value(false))
		.col_expr(
			hipcheck_runs::Column::Stderr,
			Expr::value(Some(String::new())),
		)
		.col_expr(hipcheck_runs::Column::StderrTruncated, Expr::value(false))
		.col_expr(hipcheck_runs::Column::ExitStatus, Expr::value(None::<i32>))
		.col_expr(
			hipcheck_runs::Column::ErrorKind,
			Expr::value(ASSESSMENT_INTERRUPTED_ERROR_KIND),
		)
		.col_expr(
			hipcheck_runs::Column::ErrorMessage,
			Expr::value(ASSESSMENT_INTERRUPTED_ERROR_MESSAGE),
		)
		.col_expr(hipcheck_runs::Column::Retryable, Expr::value(true))
		.filter(hipcheck_runs::Column::Status.is_in(["queued", "running"]))
		.exec(db)
		.await
		.map(|result| result.rows_affected)
}

/// Mark API-visible upgrade assessments failed after their Hipcheck run was
/// claimed as interrupted during startup reconciliation.
///
/// This is a single compare-and-update operation over rows still marked
/// `processing`, so it is idempotent and cannot overwrite a terminal result.
pub async fn reconcile_abandoned_upgrade_assessments<C: ConnectionTrait>(
	db: &C,
) -> Result<u64, DbErr> {
	let mut interrupted_assessment_ids = Query::select();
	interrupted_assessment_ids
		.column(hipcheck_runs::Column::AssessmentId)
		.from(hipcheck_runs::Entity)
		.and_where(hipcheck_runs::Column::Status.eq("failed"))
		.and_where(hipcheck_runs::Column::ErrorKind.eq(ASSESSMENT_INTERRUPTED_ERROR_KIND));

	upgrade_assessments::Entity::update_many()
		.col_expr(upgrade_assessments::Column::Status, Expr::value("failed"))
		.col_expr(
			upgrade_assessments::Column::FinishedAt,
			Expr::current_timestamp(),
		)
		.col_expr(
			upgrade_assessments::Column::Error,
			Expr::value(ASSESSMENT_INTERRUPTED_ERROR_MESSAGE),
		)
		.filter(upgrade_assessments::Column::Status.eq("processing"))
		.filter(upgrade_assessments::Column::Id.in_subquery(interrupted_assessment_ids))
		.exec(db)
		.await
		.map(|result| result.rows_affected)
}

/// Complete an existing run and add normalized evidence atomically.
pub async fn complete_hipcheck_run<C: ConnectionTrait + TransactionTrait>(
	db: &C,
	run_id: i32,
	raw_json: &str,
	report: &HipcheckReport,
	diagnostics: &HipcheckExecutionDiagnostics,
) -> Result<bool, DbErr> {
	let transaction = db.begin().await?;
	let raw_json_bytes = i32::try_from(raw_json.len()).unwrap_or(i32::MAX);
	let (raw_json, raw_json_truncated) = bound(raw_json);
	let (stdout, stdout_truncated) = bound(&diagnostics.stdout);
	let (stderr, stderr_truncated) = bound(&diagnostics.stderr);
	let completed_transition_applied = hipcheck_runs::Entity::update_many()
		.col_expr(hipcheck_runs::Column::Status, Expr::value("completed"))
		.col_expr(hipcheck_runs::Column::RawJson, Expr::value(Some(raw_json)))
		.col_expr(
			hipcheck_runs::Column::RawJsonBytes,
			Expr::value(raw_json_bytes),
		)
		.col_expr(
			hipcheck_runs::Column::RawJsonTruncated,
			Expr::value(raw_json_truncated),
		)
		.col_expr(hipcheck_runs::Column::Stdout, Expr::value(Some(stdout)))
		.col_expr(
			hipcheck_runs::Column::StdoutTruncated,
			Expr::value(stdout_truncated),
		)
		.col_expr(hipcheck_runs::Column::Stderr, Expr::value(Some(stderr)))
		.col_expr(
			hipcheck_runs::Column::StderrTruncated,
			Expr::value(stderr_truncated),
		)
		.col_expr(
			hipcheck_runs::Column::ExitStatus,
			Expr::value(diagnostics.exit_status),
		)
		.col_expr(
			hipcheck_runs::Column::ErrorKind,
			Expr::value(None::<String>),
		)
		.col_expr(
			hipcheck_runs::Column::ErrorMessage,
			Expr::value(None::<String>),
		)
		.col_expr(hipcheck_runs::Column::Retryable, Expr::value(false))
		.col_expr(
			hipcheck_runs::Column::SchemaVersion,
			Expr::value(Some(report.schema_version.clone())),
		)
		.col_expr(
			hipcheck_runs::Column::HipcheckVersion,
			Expr::value(Some(report.hipcheck.version.clone())),
		)
		.col_expr(
			hipcheck_runs::Column::HipcheckCommit,
			Expr::value(Some(report.hipcheck.commit.clone())),
		)
		.col_expr(
			hipcheck_runs::Column::TargetKind,
			Expr::value(Some(report.target.kind.clone())),
		)
		.col_expr(
			hipcheck_runs::Column::TargetPurl,
			Expr::value(report.target.purl.clone()),
		)
		.col_expr(
			hipcheck_runs::Column::SourceRepositoryUrl,
			Expr::value(Some(report.target.source_repository_url.clone())),
		)
		.col_expr(
			hipcheck_runs::Column::PolicyId,
			Expr::value(Some(report.policy.id.clone())),
		)
		.col_expr(
			hipcheck_runs::Column::PolicyVersion,
			Expr::value(report.policy.version.clone()),
		)
		.col_expr(
			hipcheck_runs::Column::PolicySource,
			Expr::value(Some(report.policy.source.clone())),
		)
		.col_expr(
			hipcheck_runs::Column::PolicyRecommendation,
			Expr::value(Some(recommendation(report))),
		)
		.filter(hipcheck_runs::Column::Id.eq(run_id))
		.filter(hipcheck_runs::Column::Status.eq("running"))
		.exec(&transaction)
		.await?
		.rows_affected
		== 1;
	if !completed_transition_applied {
		transaction.rollback().await?;
		return Ok(false);
	}
	let mut finding_ordinal = 0;
	for (ordinal, check) in report.checks.iter().enumerate() {
		finding_ordinal =
			store_check(&transaction, run_id, ordinal, finding_ordinal, check).await?;
	}
	transaction.commit().await?;
	Ok(true)
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

/// List the newest persisted assessments across every resolved version of a package.
pub async fn list_hipcheck_runs_for_package<C: ConnectionTrait>(
	db: &C,
	package_id: i32,
	limit: u64,
) -> Result<Vec<hipcheck_runs::Model>, DbErr> {
	let package_version_ids = package_versions::Entity::find()
		.filter(package_versions::Column::PackageId.eq(package_id))
		.all(db)
		.await?
		.into_iter()
		.map(|version| version.id)
		.collect::<Vec<_>>();
	if package_version_ids.is_empty() {
		return Ok(Vec::new());
	}

	hipcheck_runs::Entity::find()
		.filter(hipcheck_runs::Column::PackageVersionId.is_in(package_version_ids))
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
		assessment_id: Set(Uuid::now_v7().to_string()),
		package_version_id: Set(package_version_id),
		affected_purl: Set(None),
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
	let mut finding_ordinal = 0;
	for (ordinal, check) in report.checks.iter().enumerate() {
		finding_ordinal =
			store_check(&transaction, run.id, ordinal, finding_ordinal, check).await?;
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

/// Load a run by its externally visible UUID v7 assessment identifier.
pub async fn load_hipcheck_run_by_assessment_id<C: ConnectionTrait>(
	db: &C,
	assessment_id: &Uuid,
) -> Result<Option<StoredHipcheckRun>, DbErr> {
	let Some(run) = hipcheck_runs::Entity::find()
		.filter(hipcheck_runs::Column::AssessmentId.eq(assessment_id.to_string()))
		.one(db)
		.await?
	else {
		return Ok(None);
	};
	load_hipcheck_run(db, run.id).await
}

async fn store_check<C: ConnectionTrait>(
	db: &C,
	run_id: i32,
	check_ordinal: usize,
	finding_ordinal: i32,
	check: &HipcheckCheck,
) -> Result<i32, DbErr> {
	let check_ordinal = i32::try_from(check_ordinal).unwrap_or(i32::MAX);
	let stored = hipcheck_checks::ActiveModel {
		id: Default::default(),
		run_id: Set(run_id),
		ordinal: Set(check_ordinal),
		plugin_name: Set(check.plugin.name.clone()),
		plugin_publisher: Set(check.plugin.publisher.clone()),
		plugin_version: Set(check.plugin.version.clone()),
		plugin_query: Set(check.plugin.query.clone()),
		policy_expression: Set(check.policy.expression.clone()),
		state: Set(state(check).to_owned()),
		effect: Set(normalized_check_effect(check).to_owned()),
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
	let mut next_finding_ordinal = finding_ordinal;
	for finding in normalized_findings(check) {
		hipcheck_findings::ActiveModel {
			id: Default::default(),
			run_id: Set(run_id),
			check_id: Set(Some(stored.id)),
			ordinal: Set(next_finding_ordinal),
			kind: Set(finding.kind.to_owned()),
			effect: Set(finding.effect.to_owned()),
			severity: Set(finding.severity),
			summary: Set(bound(&finding.summary).0),
			evidence: Set(finding.evidence),
		}
		.insert(db)
		.await?;
		next_finding_ordinal = next_finding_ordinal.saturating_add(1);
	}
	Ok(next_finding_ordinal)
}

#[derive(Debug, PartialEq)]
struct NormalizedFinding {
	kind: &'static str,
	effect: &'static str,
	severity: Option<String>,
	summary: String,
	evidence: serde_json::Value,
}

fn normalized_findings(check: &HipcheckCheck) -> Vec<NormalizedFinding> {
	let finding_kind = match check.state {
		super::HipcheckCheckState::Skipped | super::HipcheckCheckState::Unsupported => {
			"missing-check"
		}
		super::HipcheckCheckState::Errored => "check-error",
		super::HipcheckCheckState::Passed | super::HipcheckCheckState::Failed => "check-result",
	};
	let mut findings = vec![NormalizedFinding {
		kind: finding_kind,
		effect: normalized_check_effect(check),
		severity: check.severity.clone(),
		summary: check.summary.clone(),
		evidence: json!({
			"plugin": {
				"publisher": check.plugin.publisher,
				"name": check.plugin.name,
				"version": check.plugin.version,
				"query": check.plugin.query,
			},
			"policy_expression": check.policy.expression,
			"state": state(check),
			"value": check.value,
			"error_kind": check.error.as_ref().map(|error| &error.kind),
			"error_retryable": check.error.as_ref().map(|error| error.retryable),
		}),
	}];
	findings.extend(
		check
			.concerns
			.iter()
			.filter(|concern| concern.kind == "missing-data")
			.map(|concern| NormalizedFinding {
				kind: "missing-evidence",
				effect: "review",
				severity: None,
				summary: concern.message.clone(),
				evidence: json!({
					"plugin": {
						"publisher": check.plugin.publisher,
						"name": check.plugin.name,
						"version": check.plugin.version,
						"query": check.plugin.query,
					},
					"concern_kind": concern.kind,
					"details": concern.details,
				}),
			}),
	);
	findings
}

fn normalized_check_effect(check: &HipcheckCheck) -> &'static str {
	match check.state {
		super::HipcheckCheckState::Skipped
		| super::HipcheckCheckState::Unsupported
		| super::HipcheckCheckState::Errored => "missing-check",
		super::HipcheckCheckState::Passed | super::HipcheckCheckState::Failed => effect(check),
	}
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
fn effect(check: &HipcheckCheck) -> &'static str {
	match check.effect {
		super::HipcheckEffect::Blocking => "blocking",
		super::HipcheckEffect::Review => "review",
		super::HipcheckEffect::Context => "context",
		super::HipcheckEffect::MissingCheck => "missing-check",
	}
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
	use super::{
		ASSESSMENT_INTERRUPTED_ERROR_KIND, ASSESSMENT_INTERRUPTED_ERROR_MESSAGE,
		MAX_STORED_HIPCHECK_OUTPUT_BYTES, bound, normalized_findings,
		reconcile_abandoned_hipcheck_runs, reconcile_abandoned_upgrade_assessments,
	};
	use crate::hipcheck::{
		HipcheckCheck, HipcheckCheckPolicy, HipcheckCheckState, HipcheckConcern, HipcheckEffect,
		HipcheckError, HipcheckPluginIdentity,
	};
	use sea_orm::{DbBackend, MockDatabase, MockExecResult, Value};
	use serde_json::json;

	#[test]
	fn output_bound_preserves_utf8_and_reports_truncation() {
		let input = format!("{}é", "x".repeat(MAX_STORED_HIPCHECK_OUTPUT_BYTES));
		let (stored, truncated) = bound(&input);
		assert!(truncated);
		assert_eq!(stored.len(), MAX_STORED_HIPCHECK_OUTPUT_BYTES);
		assert!(stored.is_char_boundary(stored.len()));
	}

	#[test]
	fn unsupported_and_plugin_errors_become_missing_check_inputs() {
		let unsupported = normalized_findings(&check(
			HipcheckCheckState::Unsupported,
			HipcheckEffect::Review,
			None,
		));
		assert_eq!(unsupported[0].kind, "missing-check");
		assert_eq!(unsupported[0].effect, "missing-check");

		let completed = normalized_findings(&check(
			HipcheckCheckState::Passed,
			HipcheckEffect::Context,
			None,
		));
		let plugin_error = normalized_findings(&check(
			HipcheckCheckState::Errored,
			HipcheckEffect::Review,
			Some(HipcheckError {
				kind: "plugin".to_owned(),
				message: "plugin process exited".to_owned(),
				retryable: true,
			}),
		));
		assert_eq!(completed[0].effect, "context");
		assert_eq!(plugin_error[0].kind, "check-error");
		assert_eq!(plugin_error[0].effect, "missing-check");
		assert_eq!(plugin_error[0].evidence["error_retryable"], true);
	}

	#[test]
	fn missing_optional_evidence_becomes_a_review_finding() {
		let mut check = check(HipcheckCheckState::Passed, HipcheckEffect::Context, None);
		check.concerns.push(HipcheckConcern {
			kind: "missing-data".to_owned(),
			message: "plugin version was not reported".to_owned(),
			details: None,
		});

		let findings = normalized_findings(&check);

		assert_eq!(findings.len(), 2);
		assert_eq!(findings[1].kind, "missing-evidence");
		assert_eq!(findings[1].effect, "review");
	}

	fn check(
		state: HipcheckCheckState,
		effect: HipcheckEffect,
		error: Option<HipcheckError>,
	) -> HipcheckCheck {
		HipcheckCheck {
			plugin: HipcheckPluginIdentity {
				name: "binary".to_owned(),
				publisher: "mitre".to_owned(),
				version: "1.0.0".to_owned(),
				query: "binary".to_owned(),
			},
			policy: HipcheckCheckPolicy {
				expression: "(lte $ 0)".to_owned(),
			},
			state,
			effect,
			severity: None,
			summary: "structured summary".to_owned(),
			value: json!({"count": 0}),
			concerns: Vec::new(),
			started_at: None,
			ended_at: None,
			error,
		}
	}

	#[tokio::test]
	async fn reconciliation_claims_queued_and_running_records_with_one_compare_and_update() {
		let db = MockDatabase::new(DbBackend::Postgres)
			.append_exec_results([MockExecResult {
				last_insert_id: 0,
				rows_affected: 2,
			}])
			.into_connection();

		assert_eq!(reconcile_abandoned_hipcheck_runs(&db).await.unwrap(), 2);

		let transaction_log = db.into_transaction_log();
		let sql = &transaction_log[0].statements()[0].sql;
		assert!(sql.contains("UPDATE \"hipcheck_runs\""));
		assert!(sql.contains("\"status\" IN"));
		assert!(sql.contains("\"status\" ="));
		let values = transaction_log[0].statements()[0]
			.values
			.as_ref()
			.expect("reconciliation update has bound values");
		assert!(values.iter().any(|value| matches!(
			value,
			Value::String(Some(value)) if value == ASSESSMENT_INTERRUPTED_ERROR_KIND
		)));
		assert!(values.iter().any(|value| matches!(
			value,
			Value::String(Some(value)) if value == ASSESSMENT_INTERRUPTED_ERROR_MESSAGE
		)));
		assert!(
			values
				.iter()
				.any(|value| matches!(value, Value::Bool(Some(true))))
		);
	}

	#[tokio::test]
	async fn reconciliation_is_idempotent_and_excludes_completed_and_failed_records() {
		let db = MockDatabase::new(DbBackend::Postgres)
			.append_exec_results([
				MockExecResult {
					last_insert_id: 0,
					rows_affected: 2,
				},
				MockExecResult {
					last_insert_id: 0,
					rows_affected: 0,
				},
			])
			.into_connection();

		assert_eq!(reconcile_abandoned_hipcheck_runs(&db).await.unwrap(), 2);
		assert_eq!(reconcile_abandoned_hipcheck_runs(&db).await.unwrap(), 0);

		let transaction_log = db.into_transaction_log();
		assert_eq!(transaction_log.len(), 2);
		for entry in transaction_log {
			let sql = &entry.statements()[0].sql;
			assert!(sql.contains("\"status\" IN"));
			let values = entry.statements()[0]
				.values
				.as_ref()
				.expect("reconciliation update has bound values");
			assert!(values.iter().any(|value| matches!(
				value,
				Value::String(Some(value)) if value == "queued"
			)));
			assert!(values.iter().any(|value| matches!(
				value,
				Value::String(Some(value)) if value == "running"
			)));
		}
	}

	#[tokio::test]
	async fn assessment_reconciliation_claims_processing_rows_for_interrupted_runs() {
		let db = MockDatabase::new(DbBackend::Postgres)
			.append_exec_results([MockExecResult {
				last_insert_id: 0,
				rows_affected: 2,
			}])
			.into_connection();

		assert_eq!(
			reconcile_abandoned_upgrade_assessments(&db).await.unwrap(),
			2
		);

		let transaction_log = db.into_transaction_log();
		let sql = &transaction_log[0].statements()[0].sql;
		assert!(sql.contains("UPDATE \"public\".\"upgrade_assessments\""));
		assert!(sql.contains("\"status\" ="));
		assert!(sql.contains("CURRENT_TIMESTAMP"));
		assert!(sql.contains("IN (SELECT"));
		assert!(sql.contains("FROM \"hipcheck_runs\""));
		let values = transaction_log[0].statements()[0]
			.values
			.as_ref()
			.expect("assessment reconciliation update has bound values");
		assert!(values.iter().any(|value| matches!(
			value,
			Value::String(Some(value)) if value == "processing"
		)));
		assert!(values.iter().any(|value| matches!(
			value,
			Value::String(Some(value)) if value == "failed"
		)));
		assert!(values.iter().any(|value| matches!(
			value,
			Value::String(Some(value)) if value == ASSESSMENT_INTERRUPTED_ERROR_KIND
		)));
		assert!(values.iter().any(|value| matches!(
			value,
			Value::String(Some(value)) if value == ASSESSMENT_INTERRUPTED_ERROR_MESSAGE
		)));
	}

	#[tokio::test]
	async fn assessment_reconciliation_is_idempotent_and_leaves_terminal_rows_unchanged() {
		let db = MockDatabase::new(DbBackend::Postgres)
			.append_exec_results([
				MockExecResult {
					last_insert_id: 0,
					rows_affected: 2,
				},
				MockExecResult {
					last_insert_id: 0,
					rows_affected: 0,
				},
			])
			.into_connection();

		assert_eq!(
			reconcile_abandoned_upgrade_assessments(&db).await.unwrap(),
			2
		);
		assert_eq!(
			reconcile_abandoned_upgrade_assessments(&db).await.unwrap(),
			0
		);

		let transaction_log = db.into_transaction_log();
		assert_eq!(transaction_log.len(), 2);
		for entry in transaction_log {
			let sql = &entry.statements()[0].sql;
			assert!(sql.contains("UPDATE \"public\".\"upgrade_assessments\""));
			assert!(sql.contains("IN (SELECT"));
			let values = entry.statements()[0]
				.values
				.as_ref()
				.expect("assessment reconciliation update has bound values");
			assert!(values.iter().any(|value| matches!(
				value,
				Value::String(Some(value)) if value == "processing"
			)));
			assert!(values.iter().any(|value| matches!(
				value,
				Value::String(Some(value)) if value == ASSESSMENT_INTERRUPTED_ERROR_KIND
			)));
		}
	}
}
