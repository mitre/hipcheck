//! Defines the API endpoints for the Night Vision server.

pub mod ctx;

mod cve_worker;
mod package_source_worker;

use crate::{api::ctx::ApiCtx, error::FatalError};
use camino::{Utf8Path, Utf8PathBuf};
// We'd prefer to use `jiff` over `chrono`, but `dropshot` depends on
// an old version of `schemars` that doesn't support `jiff`. When we
// can use a newer version of `schemars`, we should switch to using
// `jiff`.
//use chrono::Utc;
use chrono::{DateTime, Utc};
use dropshot::{
    ClientErrorStatusCode, HttpError, HttpResponseAccepted, HttpResponseOk, Path, Query,
    RequestContext, ServerBuilder, TypedBody,
};
use http::{HeaderMap, HeaderValue, header::WWW_AUTHENTICATE};
use nv_common::{
    config::Config,
    cve::kev::{
        KevContext, KevNpmMatchStatus, ReachableNpmPackageVersion,
        kev_affected_npm_package_version, kev_affected_npm_package_versions,
    },
    cve::storage::{
        has_active_kev_records, has_cve_list_records, last_successful_cve_list_sync_commit,
        latest_cve_list_sync_run, latest_successful_cve_list_sync_run,
    },
    db::entities::{
        cisa_kev_sync_runs::Model as KevSyncRun, cve_list_sync_runs::Model as CveListSyncRun,
        hipcheck_runs, package_source_versions, package_source_warnings, package_sources,
        package_versions, upgrade_assessments,
    },
    display_safety::{diagnostic_text, raw_json_preview, summary_text, url_label},
    hipcheck::{
        assessment::{execute_queued_assessment, queue_assessment, queue_assessment_with_id},
        storage::{
            load_hipcheck_run_by_assessment_id, reconcile_abandoned_hipcheck_runs,
            reconcile_abandoned_upgrade_assessments,
        },
    },
    kev::{latest_kev_sync_run, latest_successful_kev_sync_run},
    npm::{
        candidates::{
            ApiCompatibility, CandidateStatus, UpgradeCandidate, UpgradeDistance,
            api_compatibility, discover_upgrade_candidates, upgrade_distance,
            validate_explicit_candidate,
        },
        elaboration::{
            ElaborationLimits, NpmRegistryClient, PackageVersion, PackumentProvider as _,
            lifecycle::{
                CancellationOutcome, DeletionOutcome, FailureKind, delete_by_public_id,
                request_cancellation,
            },
            normalize_repository_url,
            storage::{persist_assessment_target, persisted_package_versions},
        },
        package_json::NpmPackageJson,
        purl::NpmPackagePurl,
        types::NpmPackageName,
    },
};
use nv_server_api::{
    AssessmentCheck, AssessmentDiagnostics, AssessmentEvidence, AssessmentEvidenceQuery,
    AssessmentFinding, AssessmentPathParams, AssessmentStatus, CveIngestHealth,
    CveListSyncRunHealth, DataStatus, DatasetDataStatus, DatasetSyncAttempt, Health,
    HealthDiagnostics, KevIngestHealth, KevSyncRunHealth, NvServerApi, PackageSource,
    PackageSourceAttention, PackageSourceEcosystem, PackageSourceExposure,
    PackageSourceExposureKevContext, PackageSourceExposureSummaryStatus, PackageSourceExposures,
    PackageSourceExposuresStatus, PackageSourceFailureKind, PackageSourceLifecycle,
    PackageSourceList, PackageSourceListDirection, PackageSourceListFilter, PackageSourceListQuery,
    PackageSourceListSort, PackageSourceOperationResponse, PackageSourceOperationStatus,
    PackageSourcePathParams, PackageSourceStatus, PackageSourceStatusCancelled,
    PackageSourceStatusCompleted, PackageSourceStatusCompletedWithWarnings,
    PackageSourceStatusFailed, PackageSourceStatusProcessing, PackageSourceSummary,
    PackageSourceWarning, PostAssessmentBody, PostAssessmentResponse, PostPackageSourceBody,
    PostPackageSourceResponse, PostUpgradeAssessmentBody, PostUpgradeAssessmentResponse,
    UpgradeAssessmentCandidateVersion, UpgradeAssessmentCaveat, UpgradeAssessmentError,
    UpgradeAssessmentEvidence, UpgradeAssessmentEvidenceSourceType, UpgradeAssessmentFinding,
    UpgradeAssessmentFindingCategory, UpgradeAssessmentFindingEffect, UpgradeAssessmentInput,
    UpgradeAssessmentKevLinkage, UpgradeAssessmentPackageSourceInput, UpgradeAssessmentPathParams,
    UpgradeAssessmentResult, UpgradeAssessmentStatus, UpgradeAssessmentUpgradeDistance,
    UpgradeAssessmentVerdict, UpgradeAssessmentVulnerablePackageInput,
    UpgradeAssessmentWorkflowStatus, VersionedPackage, nv_server_api_mod::api_description,
};
use percent_encoding::percent_decode_str;
use sea_orm::{
    ActiveModelTrait as _, ActiveValue::Set, ColumnTrait as _, DatabaseConnection,
    EntityTrait as _, QueryFilter as _, QueryOrder as _, QuerySelect as _, sea_query::Expr,
};
use secrecy::{ExposeSecret as _, SecretString};
use slog::Logger;
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    fs::File,
    time::Duration,
};
use subtle::ConstantTimeEq as _;
use uuid::Uuid;

/// The REST API interface.
///
/// This is a wrapper for the `dropshot` `ApiDescription` populate with our `ApiCtx` (the shared
/// data available to every endpoint handler).
pub struct RestApi(dropshot::ApiDescription<ApiCtx>);

impl RestApi {
    /// Initialize the API.
    pub fn new() -> Result<Self, FatalError> {
        api_description::<Self>()
            .map(RestApi)
            .map_err(FatalError::FailedToBuildDropshotServer)
    }

    /// Launch the server, handling requests.
    ///
    /// Returns `FatalError` if the server encounters a problem that either blocks launching or
    /// causes the server to be unable to serve more requests.
    pub async fn serve(self, config: &Config, log: Logger) -> Result<(), FatalError> {
        let api = self.0;
        let ctx = ApiCtx::init(config).await?;
        let reconciled = reconcile_assessments_on_startup(ctx.db())
            .await
            .map_err(FatalError::FailedToReconcileAssessments)?;
        if reconciled > 0 {
            slog::info!(log, "reconciled assessments interrupted by a prior server process"; "count" => reconciled);
        }
        let cve_list_worker_config = config.cve_list_worker_config();
        let _cve_list_worker = cve_worker::spawn_cve_list_worker(
            ctx.db().clone(),
            cve_list_worker_config,
            log.clone(),
        );
        let _kev_worker =
            nv_common::kev::spawn_kev_sync_worker(config, ctx.db().clone(), log.clone());
        let _package_source_worker = package_source_worker::spawn(&ctx, log.clone());

        ServerBuilder::new(api, ctx, log)
            .config(config.dropshot_config()?)
            .start()
            .map_err(FatalError::FailedToStartDropshotServer)?
            .await
            .map_err(FatalError::UnknownServerError)
    }

    /// Generate an OpenAPI Description from the
    /// dropshot::ApiDescription and write it out to the specified path.
    /// If the destination path argument is an absolute path, it is taken as-is.
    /// If it is a relative path, it is interpreted as relative to the
    /// location of the workspace root on the filesystem.
    /// The OpenAPI Description will include the current version of
    /// the `nv-server` crate.
    pub fn write_openapi(&self, dest_path: &Utf8Path) -> Result<(), FatalError> {
        let ver_string = env!("CARGO_PKG_VERSION");
        let ver = dropshot::semver::Version::parse(ver_string)
            // PANIC SAFETY: Cargo enforces semver version syntax to build, so this parse should always succeed
            .expect("Internal error: failed to parse nv-server version as semver format");
        let openapi = self.0.openapi("Night Vision", ver);

        let path = openapi_dest_path(dest_path);
        // Explicitly match on the Result instead of using `.map_err`,
        // to avoid needing to clone `path` to pass to two error
        // closures. Currently, Rust can't prove that `path` would
        // only be used once in each case, if the first path involves
        // passing `path` to a closure.
        let mut file = match File::create(&path) {
            Ok(file) => file,
            Err(e) => return Err(FatalError::FailedToCreateOpenApiDescFile(path, e)),
        };

        openapi
            .write(&mut file)
            .map_err(|e| FatalError::FailedToWriteOpenApiDescFile(path, e))?;
        Ok(())
    }
}

/// Reconcile work owned by a process that exited before recording a result.
///
/// This runs before background workers and the HTTP listener are started, so
/// every unfinished persisted assessment belongs to the preceding process.
async fn reconcile_assessments_on_startup(db: &DatabaseConnection) -> Result<u64, sea_orm::DbErr> {
    reconcile_abandoned_hipcheck_runs(db).await?;
    reconcile_abandoned_upgrade_assessments(db).await
}

/// Create the path to store an OpenAPI Description.
/// If the destination path argument is an absolute path, it is taken as-is.
/// If it is a relative path, it is interpreted as relative to the
/// location of the workspace root on the filesystem.
fn openapi_dest_path(dest_path: &Utf8Path) -> Utf8PathBuf {
    // This directory path will point to "backend/nv-server"
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let mut path = Utf8PathBuf::new();
    path.push(manifest_dir);
    // Pop the trailing `/nv-server`, to make the path relative to the
    // workspace root
    path.pop();
    // This call to `push()` may either replace the entire path (if
    // `dest_path` is absolute) or append `dest_path` (if relative)
    path.push(dest_path);

    path
}

// Implementation of the `NvServerApi` trait. This is where our actual endpoint handlers go.
//
// Note that the trait and relevant API types are defined in the `nv-server-api` crate.
impl NvServerApi for RestApi {
    type Context = ApiCtx;

    async fn health(
        _ctx: RequestContext<Self::Context>,
    ) -> Result<HttpResponseOk<Health>, HttpError> {
        Ok(HttpResponseOk(Health {
            status: "ok".to_owned(),
        }))
    }

    async fn health_diagnostics(
        ctx: RequestContext<Self::Context>,
    ) -> Result<HttpResponseOk<HealthDiagnostics>, HttpError> {
        match health_diagnostics_authorization(
            ctx.context().health_diagnostics_token(),
            ctx.request
                .headers()
                .get_all("authorization")
                .iter()
                .map(|value| value.to_str().ok()),
        ) {
            HealthDiagnosticsAuthorization::Authorized => {}
            HealthDiagnosticsAuthorization::Disabled => return Err(health_diagnostics_disabled()),
            HealthDiagnosticsAuthorization::Unauthorized => {
                return Err(health_diagnostics_unauthorized());
            }
        }
        let cve_ingest = cve_ingest_health(
            ctx.context().db(),
            ctx.context().cve_list_freshness_threshold(),
        )
        .await?;
        let kev_ingest =
            kev_ingest_health(ctx.context().db(), ctx.context().kev_freshness_threshold()).await?;

        Ok(HttpResponseOk(HealthDiagnostics {
            status: "ok".to_owned(),
            cve_ingest,
            kev_ingest,
        }))
    }

    async fn data_status(
        ctx: RequestContext<Self::Context>,
    ) -> Result<HttpResponseOk<DataStatus>, HttpError> {
        Ok(HttpResponseOk(
            data_status(
                ctx.context().db(),
                ctx.context().cve_list_freshness_threshold(),
                ctx.context().kev_freshness_threshold(),
                Utc::now(),
            )
            .await?,
        ))
    }

    async fn post_package_source(
        ctx: RequestContext<Self::Context>,
        body_param: TypedBody<PostPackageSourceBody>,
    ) -> Result<HttpResponseAccepted<PostPackageSourceResponse>, HttpError> {
        let context = ctx.context();
        let db = context.db();
        let body = body_param.into_inner();
        let file_name = body.file_name;
        let contents = body.contents;
        let display_name = derive_package_source_display_name(&file_name, &contents)?;

        // A successful submission is durable even when every active-resolution
        // slot is occupied: the source is persisted `pending` unconditionally,
        // and the background worker claims it once capacity allows. This does
        // not promise that processing has started.
        let stored = store_validated_package_source(db, display_name, file_name, contents).await?;
        Ok(HttpResponseAccepted(PostPackageSourceResponse {
            id: stored.id,
        }))
    }

    async fn list_package_sources(
        ctx: RequestContext<Self::Context>,
        query_params: Query<PackageSourceListQuery>,
    ) -> Result<HttpResponseOk<PackageSourceList>, HttpError> {
        Ok(HttpResponseOk(
            package_source_list(ctx.context().db(), query_params.into_inner()).await?,
        ))
    }

    async fn get_package_source(
        ctx: RequestContext<Self::Context>,
        path_params: Path<PackageSourcePathParams>,
    ) -> Result<HttpResponseOk<PackageSourceStatus>, HttpError> {
        let db = ctx.context().db();
        let id = path_params.into_inner().id;
        let pkg_src = lookup_package_source(db, id).await?;
        match pkg_src {
            Some(status) => Ok(HttpResponseOk(status)),
            None => Err(HttpError::for_not_found(
                None,
                format!("unknown package source {id}"),
            )),
        }
    }

    async fn cancel_package_source(
        ctx: RequestContext<Self::Context>,
        path_params: Path<PackageSourcePathParams>,
    ) -> Result<HttpResponseAccepted<PackageSourceOperationResponse>, HttpError> {
        let id = path_params.into_inner().id;
        match request_cancellation(ctx.context().db(), &id.to_string(), Utc::now())
            .await
            .map_err(|_| internal_server_error())?
        {
            CancellationOutcome::Accepted => {
                Ok(HttpResponseAccepted(PackageSourceOperationResponse {
                    id,
                    status: PackageSourceOperationStatus::Cancelled,
                }))
            }
            CancellationOutcome::Conflict => Err(HttpError::for_client_error(
                None,
                ClientErrorStatusCode::CONFLICT,
                "package source already reached a terminal result".to_owned(),
            )),
            CancellationOutcome::NotFound => Err(HttpError::for_not_found(
                None,
                format!("unknown package source {id}"),
            )),
        }
    }

    async fn delete_package_source(
        ctx: RequestContext<Self::Context>,
        path_params: Path<PackageSourcePathParams>,
    ) -> Result<HttpResponseAccepted<PackageSourceOperationResponse>, HttpError> {
        let id = path_params.into_inner().id;
        match delete_by_public_id(ctx.context().db(), &id.to_string(), "caller")
            .await
            .map_err(|_| internal_server_error())?
        {
            DeletionOutcome::Accepted => Ok(HttpResponseAccepted(PackageSourceOperationResponse {
                id,
                status: PackageSourceOperationStatus::Deleting,
            })),
            DeletionOutcome::NotFound => Err(HttpError::for_not_found(
                None,
                format!("unknown package source {id}"),
            )),
        }
    }

    async fn get_package_source_exposures(
        ctx: RequestContext<Self::Context>,
        path_params: Path<PackageSourcePathParams>,
    ) -> Result<HttpResponseOk<PackageSourceExposures>, HttpError> {
        let db = ctx.context().db();
        let id = path_params.into_inner().id;
        let package_source = lookup_package_source(db, id).await?;
        let Some(package_source) = package_source else {
            return Err(HttpError::for_not_found(
                None,
                format!("unknown package source {id}"),
            ));
        };

        let status = package_source_exposures_status(db, &package_source).await?;
        let exposures = match &package_source {
            PackageSourceStatus::Completed(completed) => {
                package_source_exposures(db, &completed.versioned_packages).await?
            }
            PackageSourceStatus::CompletedWithWarnings(completed) => {
                package_source_exposures(db, &completed.versioned_packages).await?
            }
            PackageSourceStatus::Failed(_) => Vec::new(),
            PackageSourceStatus::Cancelled(_) => Vec::new(),
            PackageSourceStatus::Pending(_) | PackageSourceStatus::Processing(_) => Vec::new(),
        };

        Ok(HttpResponseOk(PackageSourceExposures {
            id,
            status,
            exposures,
        }))
    }

    async fn post_assessment(
        ctx: RequestContext<Self::Context>,
        body_param: TypedBody<PostAssessmentBody>,
    ) -> Result<HttpResponseAccepted<PostAssessmentResponse>, HttpError> {
        let body = body_param.into_inner();
        let context = ctx.context();
        let admission = context.try_admit_hipcheck().ok_or_else(|| {
            HttpError::for_unavail(
                Some("AssessmentCapacity".to_owned()),
                "assessment capacity is exhausted".to_owned(),
            )
        })?;
        let target =
            validate_and_persist_upgrade_target(context, &body.affected_purl, &body.target_purl)
                .await?;
        let queued = queue_assessment(context.db(), &body.affected_purl, &target)
            .await
            .map_err(assessment_http_error)?;
        let id = queued.id;
        let db = context.db().clone();
        let runner = context.hipcheck_runner_config();
        let log = ctx.log.new(slog::o!("assessment_id" => id.to_string()));
        tokio::spawn(async move {
            let _admission = admission;
            if let Err(error) = execute_queued_assessment(&db, &queued, &runner).await {
                slog::error!(
                    log,
                    "assessment could not persist terminal state";
                    "failure_kind" => "terminal-state-persistence",
                    "error" => %error,
                    "error_debug" => ?error,
                );
            }
        });
        Ok(HttpResponseAccepted(PostAssessmentResponse { id }))
    }

    async fn get_assessment(
        ctx: RequestContext<Self::Context>,
        path_params: Path<AssessmentPathParams>,
    ) -> Result<HttpResponseOk<AssessmentStatus>, HttpError> {
        let id = path_params.into_inner().id;
        let stored = load_hipcheck_run_by_assessment_id(ctx.context().db(), &id)
            .await
            .map_err(|_| internal_server_error())?
            .ok_or_else(|| HttpError::for_not_found(None, format!("unknown assessment {id}")))?;
        Ok(HttpResponseOk(AssessmentStatus {
            id,
            state: stored.run.status,
            affected_purl: stored.run.affected_purl.map(|value| summary_text(&value)),
            target: stored.run.target_purl.map(|value| summary_text(&value)),
            source_repository_url: stored
                .run
                .source_repository_url
                .map(|value| url_label(&value)),
            recommendation: stored.run.policy_recommendation,
            finding_count: stored.findings.len(),
            exit_status: stored.run.exit_status,
            error_kind: stored.run.error_kind,
            error_message: stored.run.error_message.map(|value| summary_text(&value)),
            retryable: stored.run.retryable,
        }))
    }

    async fn get_assessment_evidence(
        ctx: RequestContext<Self::Context>,
        path_params: Path<AssessmentPathParams>,
        query: Query<AssessmentEvidenceQuery>,
    ) -> Result<HttpResponseOk<AssessmentEvidence>, HttpError> {
        let id = path_params.into_inner().id;
        let include_raw = query.into_inner().include_raw_hipcheck.unwrap_or(false);
        if include_raw {
            raw_hipcheck_evidence_authorization(
                ctx.context().health_diagnostics_token(),
                ctx.request
                    .headers()
                    .get_all("authorization")
                    .iter()
                    .map(|value| value.to_str().ok()),
            )?;
        }
        let stored = load_hipcheck_run_by_assessment_id(ctx.context().db(), &id)
            .await
            .map_err(|_| internal_server_error())?
            .ok_or_else(|| HttpError::for_not_found(None, format!("unknown assessment {id}")))?;
        Ok(HttpResponseOk(AssessmentEvidence {
            id,
            affected_purl: stored.run.affected_purl.as_deref().map(summary_text),
            diagnostics: assessment_diagnostics(&stored.run),
            checks: stored
                .checks
                .into_iter()
                .map(|check| AssessmentCheck {
                    state: check.state,
                    effect: check.effect,
                    summary: summary_text(&check.summary),
                })
                .collect(),
            findings: stored
                .findings
                .into_iter()
                .map(|finding| AssessmentFinding {
                    kind: finding.kind,
                    effect: finding.effect,
                    severity: finding.severity,
                    summary: summary_text(&finding.summary),
                })
                .collect(),
            raw_hipcheck: include_raw
                .then_some(stored.run.raw_json)
                .flatten()
                .map(|value| raw_json_preview(&value)),
        }))
    }

    async fn post_upgrade_assessment(
        ctx: RequestContext<Self::Context>,
        body_param: TypedBody<PostUpgradeAssessmentBody>,
    ) -> Result<HttpResponseAccepted<PostUpgradeAssessmentResponse>, HttpError> {
        let body = body_param.into_inner();
        validate_upgrade_assessment_request(&body)?;
        let id = Uuid::now_v7();
        let input = input_from_request(&body);
        let context = ctx.context();
        let status_url = format!("/upgrade-assessments/{id}");
        let result_url = format!("/upgrade-assessments/{id}/result");
        let (work, admission) = match input.candidate_version.as_deref() {
            Some(candidate_version) => {
                let admission = context.try_admit_hipcheck().ok_or_else(|| {
                    HttpError::for_unavail(
                        Some("AssessmentCapacity".to_owned()),
                        "assessment capacity is exhausted".to_owned(),
                    )
                })?;
                let vulnerable_package = input.vulnerable_package.as_ref().ok_or_else(|| {
                    upgrade_validation_error(
                        "candidateVersion requires vulnerablePackage to identify the affected dependency",
                    )
                })?;
                let package_name = NpmPackageName::parse(vulnerable_package.name.clone())
                    .map_err(invalid_upgrade_request)?;
                let affected_purl = vulnerable_package.purl.clone().unwrap_or_else(|| {
                    PackageVersion::from_npm(&package_name, &vulnerable_package.version).purl()
                });
                let target_purl = PackageVersion::from_npm(&package_name, candidate_version).purl();
                let target_purl =
                    validate_and_persist_upgrade_target(context, &affected_purl, &target_purl)
                        .await?;
                (
                    UpgradeAssessmentWork::Explicit((affected_purl, target_purl)),
                    admission,
                )
            }
            None => {
                let admission = context.try_admit_package_elaboration().ok_or_else(|| {
                    HttpError::for_unavail(
                        Some("CandidateDiscoveryCapacity".to_owned()),
                        "candidate discovery capacity is exhausted".to_owned(),
                    )
                })?;
                (UpgradeAssessmentWork::Discovered, admission)
            }
        };
        let created_at = create_upgrade_assessment(context.db(), id, &input).await?;
        let work = match work {
            UpgradeAssessmentWork::Explicit((affected_purl, target_purl)) => {
                match queue_assessment_with_id(context.db(), id, &affected_purl, &target_purl).await
                {
                    Ok(queued) => UpgradeAssessmentWork::Queued(queued),
                    Err(error) => {
                        let response_error = assessment_http_error(error);
                        let _ = mark_upgrade_assessment_failed(
                            context.db(),
                            id,
                            "assessment queue setup failed".to_owned(),
                        )
                        .await;
                        return Err(response_error);
                    }
                }
            }
            UpgradeAssessmentWork::Discovered => UpgradeAssessmentWork::Discovered,
            UpgradeAssessmentWork::Queued(_) => {
                unreachable!("only explicit input queues a Hipcheck assessment")
            }
        };

        // The row is inserted before spawning work.  Therefore a client can always
        // poll the durable processing state, even when it races this task.
        let db = context.db().clone();
        let runner = context.hipcheck_runner_config();
        let registry_url = context.npm_registry_url().clone();
        let limits = context.package_elaboration_limits();
        let max_packument_bytes = context.package_elaboration_max_packument_bytes();
        tokio::spawn(async move {
            let result = match work {
                UpgradeAssessmentWork::Queued(queued) => {
                    let _admission = admission;
                    match execute_queued_assessment(&db, &queued, &runner).await {
                        Ok(()) => {
                            complete_upgrade_assessment(
                                &db,
                                id,
                                input,
                                UpgradeAssessmentVerdict::Recommended,
                            )
                            .await
                        }
                        Err(error) => Err(HttpError::for_internal_error(error.to_string())),
                    }
                }
                UpgradeAssessmentWork::Discovered => {
                    let _admission = admission;
                    complete_discovered_upgrade_assessment(
                        &db,
                        id,
                        input,
                        registry_url,
                        limits,
                        max_packument_bytes,
                    )
                    .await
                }
                UpgradeAssessmentWork::Explicit(_) => {
                    unreachable!("explicit input must be queued before work starts")
                }
            };
            if let Err(error) = result {
                // A failure to record the failure is only possible when the database itself
                // is unavailable; the original processing row remains available for recovery.
                let _ = mark_upgrade_assessment_failed(&db, id, error.to_string()).await;
            }
        });

        Ok(HttpResponseAccepted(PostUpgradeAssessmentResponse {
            id,
            status: UpgradeAssessmentWorkflowStatus::Pending,
            created_at,
            status_url: Some(status_url),
            result_url: Some(result_url),
        }))
    }

    async fn get_upgrade_assessment(
        ctx: RequestContext<Self::Context>,
        path_params: Path<UpgradeAssessmentPathParams>,
    ) -> Result<HttpResponseOk<UpgradeAssessmentStatus>, HttpError> {
        let id = path_params.into_inner().id;
        let assessment = load_upgrade_assessment(ctx.context().db(), id).await?;
        Ok(HttpResponseOk(assessment_status(assessment)?))
    }

    async fn get_upgrade_assessment_result(
        ctx: RequestContext<Self::Context>,
        path_params: Path<UpgradeAssessmentPathParams>,
    ) -> Result<HttpResponseOk<UpgradeAssessmentResult>, HttpError> {
        let id = path_params.into_inner().id;
        let assessment = load_upgrade_assessment(ctx.context().db(), id).await?;
        Ok(HttpResponseOk(assessment_result(assessment)?))
    }
}

#[derive(Debug, PartialEq, Eq)]
enum HealthDiagnosticsAuthorization {
    Authorized,
    Disabled,
    Unauthorized,
}

fn health_diagnostics_authorization<'a>(
    configured_token: Option<&SecretString>,
    authorization_headers: impl IntoIterator<Item = Option<&'a str>>,
) -> HealthDiagnosticsAuthorization {
    let Some(configured_token) = configured_token else {
        return HealthDiagnosticsAuthorization::Disabled;
    };
    let mut authorization_headers = authorization_headers.into_iter();
    let Some(Some(authorization)) = authorization_headers.next() else {
        return HealthDiagnosticsAuthorization::Unauthorized;
    };
    if authorization_headers.next().is_some() {
        return HealthDiagnosticsAuthorization::Unauthorized;
    }
    let Some(token) = authorization.strip_prefix("Bearer ") else {
        return HealthDiagnosticsAuthorization::Unauthorized;
    };

    if token
        .as_bytes()
        .ct_eq(configured_token.expose_secret().as_bytes())
        .into()
    {
        HealthDiagnosticsAuthorization::Authorized
    } else {
        HealthDiagnosticsAuthorization::Unauthorized
    }
}

fn health_diagnostics_disabled() -> HttpError {
    HttpError::for_not_found(None, "unknown endpoint".to_owned())
}

fn health_diagnostics_unauthorized() -> HttpError {
    let mut error = HttpError::for_client_error(
        Some("HealthDiagnosticsUnauthorized".to_owned()),
        ClientErrorStatusCode::UNAUTHORIZED,
        "unauthorized".to_owned(),
    );
    let mut headers = HeaderMap::new();
    headers.insert(WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
    error.headers = Some(Box::new(headers));
    error
}

fn raw_hipcheck_evidence_disabled() -> HttpError {
    HttpError::for_not_found(None, "raw Hipcheck evidence is disabled".to_owned())
}

fn raw_hipcheck_evidence_authorization<'a>(
    configured_token: Option<&SecretString>,
    authorization_headers: impl IntoIterator<Item = Option<&'a str>>,
) -> Result<(), HttpError> {
    match health_diagnostics_authorization(configured_token, authorization_headers) {
        HealthDiagnosticsAuthorization::Authorized => Ok(()),
        HealthDiagnosticsAuthorization::Disabled => Err(raw_hipcheck_evidence_disabled()),
        HealthDiagnosticsAuthorization::Unauthorized => Err(raw_hipcheck_evidence_unauthorized()),
    }
}

fn raw_hipcheck_evidence_unauthorized() -> HttpError {
    let mut error = HttpError::for_client_error(
        Some("RawHipcheckEvidenceUnauthorized".to_owned()),
        ClientErrorStatusCode::UNAUTHORIZED,
        "unauthorized".to_owned(),
    );
    let mut headers = HeaderMap::new();
    headers.insert(WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
    error.headers = Some(Box::new(headers));
    error
}

async fn validate_and_persist_upgrade_target(
    context: &ApiCtx,
    affected_purl: &str,
    target_purl: &str,
) -> Result<String, HttpError> {
    let affected = NpmPackagePurl::parse(affected_purl).map_err(invalid_upgrade_request)?;
    let target = NpmPackagePurl::parse(target_purl).map_err(invalid_upgrade_request)?;
    if affected.name != target.name {
        return Err(upgrade_validation_error(
            "affected and target PURLs must name the same npm package",
        ));
    }
    let baseline_matches =
        kev_affected_npm_package_version(context.db(), reachable_package(&affected, affected_purl))
            .await
            .map_err(|error| HttpError::for_internal_error(error.to_string()))?;
    if !baseline_matches
        .iter()
        .any(|matched| matched.status == KevNpmMatchStatus::Affected)
    {
        return Err(upgrade_validation_error(
            "affected PURL has no locally known active KEV match",
        ));
    }
    let client = NpmRegistryClient::new(
        context.npm_registry_url().clone(),
        context.package_elaboration_max_packument_bytes(),
        context.package_elaboration_limits().request_timeout,
    )
    .map_err(|_| upgrade_validation_error("invalid NPM registry configuration"))?;
    let packument = client.fetch(&affected.name).await.map_err(|error| {
        upgrade_validation_error(&format!("failed to fetch NPM package metadata: {error}"))
    })?;
    let candidate = validate_explicit_candidate(
        &packument,
        affected.name.as_str(),
        &affected.version,
        &target.version,
    )
    .map_err(|error| upgrade_validation_error(&error.to_string()))?;
    if !matches!(candidate.status, CandidateStatus::Included) {
        return Err(upgrade_validation_error(
            "target PURL is an excluded upgrade candidate",
        ));
    }
    let target_matches =
        kev_affected_npm_package_version(context.db(), reachable_package(&target, target_purl))
            .await
            .map_err(|error| HttpError::for_internal_error(error.to_string()))?;
    if target_matches
        .iter()
        .any(|matched| matched.status == KevNpmMatchStatus::Affected)
    {
        return Err(upgrade_validation_error(
            "target PURL matches a locally known active KEV vulnerability",
        ));
    }
    let repository = repository_for_assessment_target(
        &client,
        &affected.name,
        &affected.version,
        &target.version,
        &packument,
        &candidate,
    )
    .await?;
    let target_release = PackageVersion::from_npm(&target.name, &target.version);
    persist_assessment_target(context.db(), &target_release, repository)
        .await
        .map_err(|error| HttpError::for_internal_error(error.to_string()))?;
    Ok(target_release.purl())
}

async fn repository_for_assessment_target(
    client: &NpmRegistryClient,
    package_name: &NpmPackageName,
    affected_version: &str,
    target_version: &str,
    compact_packument: &nv_common::npm::packument::NpmPackument,
    compact_candidate: &nv_common::npm::candidates::UpgradeCandidate,
) -> Result<Option<String>, HttpError> {
    let repository = repository_for_candidate(compact_packument, compact_candidate);
    let Some(repository) = repository else {
        let full_packument = client.fetch_full(package_name).await.map_err(|error| {
            upgrade_validation_error(&format!("failed to fetch NPM package metadata: {error}"))
        })?;
        let full_candidate = validate_explicit_candidate(
            &full_packument,
            package_name.as_str(),
            affected_version,
            target_version,
        )
        .map_err(|error| upgrade_validation_error(&error.to_string()))?;
        if !matches!(full_candidate.status, CandidateStatus::Included) {
            return Err(upgrade_validation_error(
                "target PURL is an excluded upgrade candidate",
            ));
        }
        return Ok(repository_for_candidate(&full_packument, &full_candidate));
    };
    Ok(Some(repository))
}

fn repository_for_candidate(
    packument: &nv_common::npm::packument::NpmPackument,
    candidate: &nv_common::npm::candidates::UpgradeCandidate,
) -> Option<String> {
    source_repository_url(
        packument
            .versions
            .get(&candidate.version)
            .and_then(|version| version.repository.as_ref()),
        packument.repository.as_ref(),
    )
}

/// Select a usable source repository, preferring version-specific metadata.
///
/// NPM package metadata can contain a repository for both a specific version
/// and the package as a whole. An unusable version-specific value must not
/// prevent a usable package-level value from being considered.
fn source_repository_url(
    version_repository: Option<&nv_common::npm::packument::Repository>,
    package_repository: Option<&nv_common::npm::packument::Repository>,
) -> Option<String> {
    let version_repository =
        version_repository.and_then(|repository| normalize_repository_url(&repository.url));
    let package_repository =
        package_repository.and_then(|repository| normalize_repository_url(&repository.url));
    version_repository.or(package_repository)
}

fn reachable_package(package: &NpmPackagePurl, purl: &str) -> ReachableNpmPackageVersion {
    ReachableNpmPackageVersion {
        package_name: package.name.as_str().to_owned(),
        version: package.version.clone(),
        source_evidence: format!("assessment upgrade PURL {purl}"),
    }
}

fn invalid_upgrade_request(error: impl std::fmt::Display) -> HttpError {
    upgrade_validation_error(&error.to_string())
}

fn upgrade_validation_error(message: &str) -> HttpError {
    HttpError::for_bad_request(
        Some("InvalidUpgradeAssessment".to_owned()),
        message.to_owned(),
    )
}

/// Work selected for an upgrade assessment after its request has been validated.
///
/// Explicit candidates retain the existing Hipcheck workflow. Discovery has no
/// single target to hand to Hipcheck, so it evaluates every usable release
/// against the locally ingested KEV data and persists that complete result.
enum UpgradeAssessmentWork {
    Explicit((String, String)),
    Queued(nv_common::hipcheck::assessment::QueuedAssessment),
    Discovered,
}

/// A registry candidate together with the vulnerability evidence used to
/// assign its initial verdict.
#[derive(Debug)]
struct DiscoveredAssessmentCandidate {
    candidate: UpgradeCandidate,
    base_verdict: UpgradeAssessmentVerdict,
}

/// Discover every usable release newer than the input version and evaluate its
/// locally known KEV status. Pre-release and deprecated releases are omitted:
/// they remain visible to the discovery library as excluded candidates, but
/// are not applicable automatic upgrade targets.
async fn discover_assessment_candidates(
    db: &DatabaseConnection,
    input: &UpgradeAssessmentInput,
    registry_url: url::Url,
    limits: &ElaborationLimits,
    max_packument_bytes: usize,
) -> Result<Vec<DiscoveredAssessmentCandidate>, HttpError> {
    let vulnerable_package = input.vulnerable_package.as_ref().ok_or_else(|| {
        upgrade_validation_error(
            "vulnerablePackage is required for automatic discovery so the affected dependency can be identified",
        )
    })?;
    let package_name =
        NpmPackageName::parse(vulnerable_package.name.clone()).map_err(invalid_upgrade_request)?;
    let affected_purl = PackageVersion::from_npm(&package_name, &vulnerable_package.version).purl();
    let affected = NpmPackagePurl::parse(&affected_purl).map_err(invalid_upgrade_request)?;
    let client = NpmRegistryClient::new(registry_url, max_packument_bytes, limits.request_timeout)
        .map_err(|_| upgrade_validation_error("invalid NPM registry configuration"))?;
    let packument = client.fetch(&package_name).await.map_err(|error| {
        upgrade_validation_error(&format!("failed to fetch NPM package metadata: {error}"))
    })?;
    validate_discovery_packument_identity(&package_name, &packument.name)?;
    let candidates = discover_upgrade_candidates(&packument, &vulnerable_package.version)
        .map_err(|error| upgrade_validation_error(&error.to_string()))?;

    let candidates: Vec<_> = candidates
        .into_iter()
        .filter(|candidate| matches!(candidate.status, CandidateStatus::Included))
        .collect();
    let mut reachable = vec![reachable_package(&affected, &affected_purl)];
    reachable.extend(candidates.iter().map(|candidate| {
        let target_purl = PackageVersion::from_npm(&package_name, &candidate.version).purl();
        reachable_package_for_validated_purl(&target_purl)
    }));
    let affected_versions: HashSet<_> = kev_affected_npm_package_versions(db, &reachable)
        .await
        .map_err(|error| HttpError::for_internal_error(error.to_string()))?
        .into_iter()
        .filter(|matched| matched.status == KevNpmMatchStatus::Affected)
        .filter_map(|matched| matched.affected_version)
        .collect();
    if !affected_versions.contains(&vulnerable_package.version) {
        return Err(upgrade_validation_error(
            "affected PURL has no locally known active KEV match",
        ));
    }

    let mut evaluated = Vec::new();
    for candidate in candidates {
        let affected = affected_versions.contains(&candidate.version.to_string());
        let (base_verdict, _caveat) = if affected {
            (
                UpgradeAssessmentVerdict::Avoid,
                "Candidate matches a locally known active KEV vulnerability.".to_owned(),
            )
        } else {
            (
                UpgradeAssessmentVerdict::Recommended,
                "Candidate has no locally known active KEV vulnerability.".to_owned(),
            )
        };
        evaluated.push(DiscoveredAssessmentCandidate {
            candidate,
            base_verdict,
        });
    }
    Ok(evaluated)
}

fn reachable_package_for_validated_purl(purl: &str) -> ReachableNpmPackageVersion {
    let package = NpmPackagePurl::parse(purl).expect("generated NPM PURL must be valid");
    reachable_package(&package, purl)
}

fn validate_discovery_packument_identity(
    requested: &NpmPackageName,
    returned: &NpmPackageName,
) -> Result<(), HttpError> {
    if requested == returned {
        Ok(())
    } else {
        Err(upgrade_validation_error(
            "NPM registry metadata does not match the requested package",
        ))
    }
}

fn assessment_diagnostics(run: &hipcheck_runs::Model) -> AssessmentDiagnostics {
    AssessmentDiagnostics {
        source_repository_url: run.source_repository_url.as_deref().map(url_label),
        stdout: run.stdout.as_deref().map(diagnostic_text),
        stdout_truncated: run.stdout_truncated,
        stderr: run.stderr.as_deref().map(diagnostic_text),
        stderr_truncated: run.stderr_truncated,
        exit_status: run.exit_status,
        error_kind: run.error_kind.clone(),
        error_message: run.error_message.as_deref().map(summary_text),
        retryable: run.retryable,
    }
}

fn assessment_http_error(error: nv_common::hipcheck::assessment::AssessmentError) -> HttpError {
    match error {
        nv_common::hipcheck::assessment::AssessmentError::UnknownPurl(_) => {
            HttpError::for_bad_request(
                Some("UnknownAssessmentPurl".to_owned()),
                "PURL is not an elaborated package version".to_owned(),
            )
        }
        nv_common::hipcheck::assessment::AssessmentError::Database(_)
        | nv_common::hipcheck::assessment::AssessmentError::TerminalOutcomeNotPersisted(_) => {
            internal_server_error()
        }
    }
}

fn internal_server_error() -> HttpError {
    HttpError::for_internal_error("internal server error".to_owned())
}

fn invalid_package_source_contents() -> HttpError {
    HttpError::for_bad_request(
        Some("InvalidPackageSourceRequest".to_owned()),
        "package-source contents are invalid".to_owned(),
    )
}

fn derive_package_source_display_name(
    file_name: &str,
    contents: &str,
) -> Result<String, HttpError> {
    let package_json = NpmPackageJson::parse_package_json(contents.as_bytes())
        .map_err(|_| invalid_package_source_contents())?;
    let display_name = package_json
        .name
        .map_or_else(|| file_name.to_owned(), |name| name.to_string());
    validated_package_source_display_name(display_name)
}

async fn store_validated_package_source(
    db: &DatabaseConnection,
    display_name: String,
    file_name: String,
    contents: String,
) -> Result<StoredPackageSource, HttpError> {
    let id = Uuid::now_v7();
    let source = nv_common::db::entities::package_sources::ActiveModel {
        source_id: Set(id.to_string()),
        display_name: Set(display_name),
        file_name: Set(file_name),
        file_contents: Set(contents),
        inferred_type: Set("npm-package-json".to_owned()),
        resolution_status: Set("pending".to_owned()),
        resolution_error: Set(None),
        ..Default::default()
    }
    .insert(db)
    .await
    .map_err(|_| internal_server_error())?;
    let _ = source;
    Ok(StoredPackageSource { id })
}

const MAX_PACKAGE_SOURCE_DISPLAY_NAME_CHARS: usize = 120;
const DEFAULT_PACKAGE_SOURCE_LIST_LIMIT: u32 = 25;
const MAX_PACKAGE_SOURCE_LIST_LIMIT: u32 = 100;
const MAX_PACKAGE_SOURCE_LIST_CURSOR: usize = 10_000;
const MAX_PACKAGE_SOURCE_LIST_QUERY_CHARS: usize = 100;

fn validated_package_source_display_name(value: String) -> Result<String, HttpError> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > MAX_PACKAGE_SOURCE_DISPLAY_NAME_CHARS {
        return Err(HttpError::for_bad_request(
            Some("InvalidPackageSourceRequest".to_owned()),
            "package-source display name is invalid".to_owned(),
        ));
    }
    Ok(value.to_owned())
}

/// Build bounded package-source summaries.
async fn package_source_list(
    db: &DatabaseConnection,
    query: PackageSourceListQuery,
) -> Result<PackageSourceList, HttpError> {
    let limit = query.limit.unwrap_or(DEFAULT_PACKAGE_SOURCE_LIST_LIMIT);
    if limit == 0 || limit > MAX_PACKAGE_SOURCE_LIST_LIMIT {
        return Err(HttpError::for_bad_request(
            Some("InvalidPackageSourceListQuery".to_owned()),
            "package-source list limit must be between 1 and 100".to_owned(),
        ));
    }
    let cursor = parse_package_source_list_cursor(query.cursor.as_deref())?;
    let source_query = query
        .query
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if source_query.is_some_and(|value| value.chars().count() > MAX_PACKAGE_SOURCE_LIST_QUERY_CHARS)
    {
        return Err(HttpError::for_bad_request(
            Some("InvalidPackageSourceListQuery".to_owned()),
            "package-source list query is too long".to_owned(),
        ));
    }

    let filter = query.filter.unwrap_or(PackageSourceListFilter::All);
    let sort = query.sort.unwrap_or(PackageSourceListSort::Activity);
    let direction = query.direction.unwrap_or(PackageSourceListDirection::Desc);

    if package_source_list_uses_database_path(filter, sort) {
        return package_source_list_sql_backed(
            db,
            cursor,
            limit,
            source_query,
            filter,
            sort,
            direction,
        )
        .await;
    }

    package_source_list_in_memory(db, cursor, limit, source_query, filter, sort, direction).await
}

async fn package_source_list_in_memory(
    db: &DatabaseConnection,
    cursor: usize,
    limit: u32,
    source_query: Option<&str>,
    filter: PackageSourceListFilter,
    sort: PackageSourceListSort,
    direction: PackageSourceListDirection,
) -> Result<PackageSourceList, HttpError> {
    let mut sources = package_sources::Entity::find()
        .filter(package_sources::Column::ResolutionStatus.ne("deleting"))
        .all(db)
        .await
        .map_err(internal_error)?;
    if let Some(source_query) = source_query {
        let source_query = source_query.to_lowercase();
        sources.retain(|source| source.display_name.to_lowercase().contains(&source_query));
    }

    let summaries = package_source_summaries(db, sources).await?;
    let mut summaries = summaries
        .into_iter()
        .filter(|summary| package_source_summary_matches_filter(summary, filter))
        .collect::<Vec<_>>();
    sort_package_source_summaries(&mut summaries, sort, direction);

    let cursor = cursor.min(summaries.len());
    let end = cursor.saturating_add(usize::try_from(limit).expect("u32 limit fits usize"));
    let items = summaries[cursor..summaries.len().min(end)].to_vec();
    let next_cursor = (end < summaries.len()).then(|| end.to_string());
    Ok(PackageSourceList { items, next_cursor })
}

fn package_source_list_uses_database_path(
    filter: PackageSourceListFilter,
    sort: PackageSourceListSort,
) -> bool {
    matches!(
        (filter, sort),
        (
            PackageSourceListFilter::All
                | PackageSourceListFilter::Processing
                | PackageSourceListFilter::Failed,
            PackageSourceListSort::Activity
                | PackageSourceListSort::Identity
                | PackageSourceListSort::ResolutionTime
        )
    )
}

async fn package_source_list_sql_backed(
    db: &DatabaseConnection,
    cursor: usize,
    limit: u32,
    source_query: Option<&str>,
    filter: PackageSourceListFilter,
    sort: PackageSourceListSort,
    direction: PackageSourceListDirection,
) -> Result<PackageSourceList, HttpError> {
    let mut query = package_sources::Entity::find();
    query = query.filter(package_sources::Column::ResolutionStatus.ne("deleting"));

    if let Some(source_query) = source_query {
        let source_query = format!("%{}%", source_query.to_lowercase());
        query = query.filter(Expr::cust_with_values(
            r#"LOWER("package_sources"."display_name") LIKE ?"#,
            [source_query],
        ));
    }

    query = match filter {
        PackageSourceListFilter::All => query,
        PackageSourceListFilter::Processing => {
            query.filter(package_sources::Column::ResolutionStatus.is_in(["pending", "processing"]))
        }
        PackageSourceListFilter::Failed => {
            query.filter(package_sources::Column::ResolutionStatus.eq("failed"))
        }
        PackageSourceListFilter::NeedsAttention => {
            unreachable!(
                "package_source_list_uses_database_path must keep aggregate-heavy filters on the in-memory path"
            )
        }
    };

    query = order_package_source_list_query(query, sort, direction);

    let page_size = u64::from(limit);
    let mut sources = query
        .offset(u64::try_from(cursor).expect("cursor fits u64"))
        .limit(page_size + 1)
        .all(db)
        .await
        .map_err(internal_error)?;
    let has_more = sources.len() > usize::try_from(limit).expect("u32 limit fits usize");
    sources.truncate(usize::try_from(limit).expect("u32 limit fits usize"));

    let items = package_source_summaries(db, sources).await?;
    let next_cursor = has_more.then(|| {
        cursor
            .saturating_add(usize::try_from(limit).expect("u32 limit fits usize"))
            .to_string()
    });
    Ok(PackageSourceList { items, next_cursor })
}

fn order_package_source_list_query(
    mut query: sea_orm::Select<package_sources::Entity>,
    sort: PackageSourceListSort,
    direction: PackageSourceListDirection,
) -> sea_orm::Select<package_sources::Entity> {
    match (sort, direction) {
        (PackageSourceListSort::Identity, PackageSourceListDirection::Asc) => {
            query = query
                .order_by_asc(package_sources::Column::DisplayName)
                .order_by_asc(package_sources::Column::SourceId);
        }
        (PackageSourceListSort::Identity, PackageSourceListDirection::Desc) => {
            query = query
                .order_by_desc(package_sources::Column::DisplayName)
                .order_by_desc(package_sources::Column::SourceId);
        }
        (PackageSourceListSort::Activity, PackageSourceListDirection::Asc) => {
            query = query
                .order_by_asc(Expr::cust(
                    r#"COALESCE("package_sources"."terminal_at", "package_sources"."created_at")"#,
                ))
                .order_by_asc(package_sources::Column::SourceId);
        }
        (PackageSourceListSort::Activity, PackageSourceListDirection::Desc) => {
            query = query
                .order_by_desc(Expr::cust(
                    r#"COALESCE("package_sources"."terminal_at", "package_sources"."created_at")"#,
                ))
                .order_by_desc(package_sources::Column::SourceId);
        }
        (PackageSourceListSort::ResolutionTime, PackageSourceListDirection::Asc) => {
            query = query
                .order_by_asc(Expr::cust(r#""package_sources"."terminal_at" IS NOT NULL"#))
                .order_by_asc(package_sources::Column::TerminalAt)
                .order_by_asc(package_sources::Column::SourceId);
        }
        (PackageSourceListSort::ResolutionTime, PackageSourceListDirection::Desc) => {
            query = query
                .order_by_desc(Expr::cust(r#""package_sources"."terminal_at" IS NOT NULL"#))
                .order_by_desc(package_sources::Column::TerminalAt)
                .order_by_desc(package_sources::Column::SourceId);
        }
        _ => {}
    }
    query
}

fn parse_package_source_list_cursor(value: Option<&str>) -> Result<usize, HttpError> {
    let Some(value) = value else { return Ok(0) };
    let cursor = value
        .parse::<usize>()
        .ok()
        .filter(|value| *value <= MAX_PACKAGE_SOURCE_LIST_CURSOR);
    cursor.ok_or_else(|| {
        HttpError::for_bad_request(
            Some("InvalidPackageSourceListQuery".to_owned()),
            "package-source list cursor is invalid".to_owned(),
        )
    })
}

struct StoredPackageSource {
    id: Uuid,
}

async fn lookup_package_source(
    db: &DatabaseConnection,
    id: Uuid,
) -> Result<Option<PackageSourceStatus>, HttpError> {
    use nv_common::db::entities::{package_source_warnings, package_sources};
    const MAX_API_WARNINGS: u64 = 100;

    let Some(source) = package_sources::Entity::find()
        .filter(package_sources::Column::SourceId.eq(id.to_string()))
        .one(db)
        .await
        .map_err(|_| internal_server_error())?
    else {
        return Ok(None);
    };
    let created_at = source.created_at.with_timezone(&Utc);
    let terminal_at = source
        .terminal_at
        .unwrap_or(source.created_at)
        .with_timezone(&Utc);
    let failure = match source.resolution_status.as_str() {
        "pending" => {
            return Ok(Some(PackageSourceStatus::Pending(
                PackageSourceStatusProcessing {
                    id,
                    created_at,
                    attempt: source.attempt_generation,
                },
            )));
        }
        "processing" => {
            return Ok(Some(PackageSourceStatus::Processing(
                PackageSourceStatusProcessing {
                    id,
                    created_at,
                    attempt: source.attempt_generation,
                },
            )));
        }
        "cancelled" => {
            let cancelled_at = source
                .terminal_at
                .unwrap_or(source.created_at)
                .with_timezone(&Utc);
            return Ok(Some(PackageSourceStatus::Cancelled(
                PackageSourceStatusCancelled {
                    id,
                    created_at,
                    cancelled_at,
                    attempt: source.attempt_generation,
                },
            )));
        }
        "deleting" => return Ok(None),
        "failed" => Some(
            FailureKind::from_stored(
                source
                    .failure_kind
                    .as_deref()
                    .ok_or_else(internal_server_error)?,
            )
            .ok_or_else(internal_server_error)?,
        ),
        "completed" => None,
        status => {
            return Err(HttpError::for_internal_error(format!(
                "unknown package source resolution status {status:?}"
            )));
        }
    };

    let versioned_packages = persisted_package_versions(db, source.id)
        .await
        .map_err(|_| internal_server_error())?
        .into_iter()
        .map(|version| VersionedPackage {
            id: Uuid::from_u64_pair(
                u64::try_from(source.id).expect("database source IDs are nonnegative"),
                u64::try_from(version.id).expect("database package-version IDs are nonnegative"),
            ),
            name: npm_package_name_from_purl(&version.package_url),
            version: version.version,
            ecosystem: PackageSourceEcosystem::Npm,
            purl: version.package_url,
            derivations: version
                .derivations
                .into_iter()
                .map(|derivation| {
                    std::iter::once("<root>".to_owned())
                        .chain(derivation)
                        .collect()
                })
                .collect(),
        })
        .collect();

    if let Some(kind) = failure {
        return Ok(Some(PackageSourceStatus::Failed(
            PackageSourceStatusFailed {
                id,
                created_at,
                finished_at: terminal_at,
                attempt: source.attempt_generation,
                diagnostic: kind.diagnostic().to_owned(),
                kind: match kind {
                    FailureKind::Validation => PackageSourceFailureKind::Validation,
                    FailureKind::DependencyUnavailable => {
                        PackageSourceFailureKind::DependencyUnavailable
                    }
                    FailureKind::Resolution => PackageSourceFailureKind::Resolution,
                    FailureKind::Internal => PackageSourceFailureKind::Internal,
                },
                retryable: source.retryable && kind.retryable(),
                previous_versioned_packages: versioned_packages,
            },
        )));
    }

    let warnings = package_source_warnings::Entity::find()
        .filter(package_source_warnings::Column::SourceId.eq(source.id))
        .order_by_asc(package_source_warnings::Column::Id)
        .limit(MAX_API_WARNINGS + 1)
        .all(db)
        .await
        .map_err(|_| internal_server_error())?;
    if warnings.is_empty() {
        return Ok(Some(PackageSourceStatus::Completed(
            PackageSourceStatusCompleted {
                id,
                created_at,
                completed_at: terminal_at,
                attempt: source.attempt_generation,
                source: package_source(source.display_name, source.file_name, source.file_contents),
                versioned_packages,
            },
        )));
    }

    let max_api_warnings =
        usize::try_from(MAX_API_WARNINGS).expect("API warning limit must fit in usize");
    let warnings_truncated = warnings.len() > max_api_warnings;
    let warnings = warnings
        .into_iter()
        .take(max_api_warnings)
        .map(|warning| PackageSourceWarning {
            declared_by_purl: warning.declared_by_purl,
            dependency_name: warning.dependency_name,
            specification_kind: warning.specification_kind,
            message: warning.message,
        })
        .collect();
    Ok(Some(PackageSourceStatus::CompletedWithWarnings(
        PackageSourceStatusCompletedWithWarnings {
            id,
            created_at,
            completed_at: terminal_at,
            attempt: source.attempt_generation,
            source: package_source(source.display_name, source.file_name, source.file_contents),
            versioned_packages,
            warnings,
            warnings_truncated,
        },
    )))
}

fn package_source(display_name: String, file_name: String, contents: String) -> PackageSource {
    PackageSource {
        display_name,
        ecosystem: PackageSourceEcosystem::Npm,
        file_name,
        contents,
    }
}

async fn package_source_summaries(
    db: &DatabaseConnection,
    sources: Vec<package_sources::Model>,
) -> Result<Vec<PackageSourceSummary>, HttpError> {
    if sources.is_empty() {
        return Ok(Vec::new());
    }
    let source_ids = sources.iter().map(|source| source.id).collect::<Vec<_>>();
    let warning_source_ids = package_source_warnings::Entity::find()
        .filter(package_source_warnings::Column::SourceId.is_in(source_ids.clone()))
        .all(db)
        .await
        .map_err(internal_error)?
        .into_iter()
        .map(|warning| warning.source_id)
        .collect::<HashSet<_>>();
    let associations = package_source_versions::Entity::find()
        .filter(package_source_versions::Column::SourceId.is_in(source_ids.clone()))
        .all(db)
        .await
        .map_err(internal_error)?;
    let package_version_ids = associations
        .iter()
        .map(|association| association.package_version_id)
        .collect::<Vec<_>>();
    let package_versions_by_id = package_versions::Entity::find()
        .filter(package_versions::Column::Id.is_in(package_version_ids))
        .all(db)
        .await
        .map_err(internal_error)?
        .into_iter()
        .map(|version| (version.id, version))
        .collect::<BTreeMap<_, _>>();
    let mut reachable_by_source = BTreeMap::<i32, BTreeSet<(String, String)>>::new();
    for association in associations {
        if let Some(version) = package_versions_by_id.get(&association.package_version_id) {
            reachable_by_source
                .entry(association.source_id)
                .or_default()
                .insert((
                    npm_package_name_from_purl(&version.package_url),
                    version.version.clone(),
                ));
        }
    }

    let completed_source_ids = sources
        .iter()
        .filter(|source| source.resolution_status == "completed")
        .map(|source| source.id)
        .collect::<HashSet<_>>();
    let exposure_data_available = has_cve_list_records(db).await.map_err(internal_error)?
        && has_active_kev_records(db).await.map_err(internal_error)?;
    let all_reachable = reachable_by_source
        .values()
        .flatten()
        .map(|(name, version)| ReachableNpmPackageVersion {
            package_name: name.clone(),
            version: version.clone(),
            source_evidence: format!("pkg:npm/{name}@{version}"),
        })
        .collect::<Vec<_>>();
    let affected = if exposure_data_available && !all_reachable.is_empty() {
        kev_affected_npm_package_versions(db, &all_reachable)
            .await
            .map_err(internal_error)?
            .into_iter()
            .filter(|matched| matched.status == KevNpmMatchStatus::Affected)
            .filter_map(|matched| {
                Some((
                    matched.package_name?,
                    matched.affected_version?,
                    matched.cve_id,
                ))
            })
            .collect::<HashSet<_>>()
    } else {
        HashSet::new()
    };

    sources
        .into_iter()
        .map(|source| {
            let id = Uuid::parse_str(&source.source_id).map_err(internal_error)?;
            let mut lifecycle = package_source_lifecycle(&source.resolution_status)?;
            if lifecycle == PackageSourceLifecycle::Completed
                && warning_source_ids.contains(&source.id)
            {
                lifecycle = PackageSourceLifecycle::CompletedWithWarnings;
            }
            let completed = completed_source_ids.contains(&source.id);
            let reachable = reachable_by_source.get(&source.id);
            let reachable_package_count = matches!(
                lifecycle,
                PackageSourceLifecycle::Completed
                    | PackageSourceLifecycle::CompletedWithWarnings
                    | PackageSourceLifecycle::Failed
            )
            .then(|| reachable.map_or(0, BTreeSet::len) as u64);
            let exposure_status = match lifecycle {
                PackageSourceLifecycle::Pending | PackageSourceLifecycle::Processing => {
                    PackageSourceExposureSummaryStatus::Processing
                }
                PackageSourceLifecycle::Failed => PackageSourceExposureSummaryStatus::Failed,
                PackageSourceLifecycle::Cancelled => PackageSourceExposureSummaryStatus::Cancelled,
                PackageSourceLifecycle::Completed
                | PackageSourceLifecycle::CompletedWithWarnings
                    if exposure_data_available =>
                {
                    PackageSourceExposureSummaryStatus::Available
                }
                PackageSourceLifecycle::Completed
                | PackageSourceLifecycle::CompletedWithWarnings => {
                    PackageSourceExposureSummaryStatus::Unavailable
                }
            };
            let exposure_count = (completed && exposure_data_available).then(|| {
                reachable.map_or(0, |versions| {
                    versions
                        .iter()
                        .flat_map(|(name, version)| {
                            affected
                                .iter()
                                .filter(move |(affected_name, affected_version, _)| {
                                    affected_name == name && affected_version == version
                                })
                        })
                        .count() as u64
                })
            });
            let attention = if lifecycle == PackageSourceLifecycle::Failed {
                PackageSourceAttention::Failed
            } else if warning_source_ids.contains(&source.id) {
                PackageSourceAttention::Warnings
            } else if matches!(
                exposure_status,
                PackageSourceExposureSummaryStatus::Unavailable
            ) {
                PackageSourceAttention::ExposureDataUnavailable
            } else {
                PackageSourceAttention::None
            };
            let created_at = utc(source.created_at);
            let resolution_at = source.terminal_at.map(utc);
            Ok(PackageSourceSummary {
                id,
                display_name: source.display_name,
                ecosystem: PackageSourceEcosystem::Npm,
                lifecycle,
                created_at,
                activity_at: resolution_at.unwrap_or(created_at),
                resolution_at,
                reachable_package_count,
                exposure_count,
                exposure_status,
                attention,
            })
        })
        .collect()
}

fn package_source_lifecycle(value: &str) -> Result<PackageSourceLifecycle, HttpError> {
    match value {
        "pending" => Ok(PackageSourceLifecycle::Pending),
        "processing" => Ok(PackageSourceLifecycle::Processing),
        "completed" => Ok(PackageSourceLifecycle::Completed),
        "failed" => Ok(PackageSourceLifecycle::Failed),
        "cancelled" => Ok(PackageSourceLifecycle::Cancelled),
        "deleting" => Err(internal_server_error()),
        _ => Err(internal_server_error()),
    }
}

fn package_source_summary_matches_filter(
    summary: &PackageSourceSummary,
    filter: PackageSourceListFilter,
) -> bool {
    match filter {
        PackageSourceListFilter::All => true,
        PackageSourceListFilter::NeedsAttention => {
            summary.attention != PackageSourceAttention::None
        }
        PackageSourceListFilter::Processing => matches!(
            summary.lifecycle,
            PackageSourceLifecycle::Pending | PackageSourceLifecycle::Processing
        ),
        PackageSourceListFilter::Failed => summary.lifecycle == PackageSourceLifecycle::Failed,
    }
}

fn sort_package_source_summaries(
    summaries: &mut [PackageSourceSummary],
    sort: PackageSourceListSort,
    direction: PackageSourceListDirection,
) {
    summaries.sort_by(|left, right| {
        let order = match sort {
            PackageSourceListSort::Activity => left.activity_at.cmp(&right.activity_at),
            PackageSourceListSort::Identity => left.display_name.cmp(&right.display_name),
            PackageSourceListSort::Lifecycle => {
                format!("{:?}", left.lifecycle).cmp(&format!("{:?}", right.lifecycle))
            }
            PackageSourceListSort::ResolutionTime => left.resolution_at.cmp(&right.resolution_at),
            PackageSourceListSort::ReachablePackages => left
                .reachable_package_count
                .cmp(&right.reachable_package_count),
            PackageSourceListSort::Exposures => left.exposure_count.cmp(&right.exposure_count),
        }
        .then_with(|| left.id.cmp(&right.id));
        match direction {
            PackageSourceListDirection::Asc => order,
            PackageSourceListDirection::Desc => order.reverse(),
        }
    });
}

async fn package_source_exposures_status(
    db: &DatabaseConnection,
    status: &PackageSourceStatus,
) -> Result<PackageSourceExposuresStatus, HttpError> {
    match status {
        PackageSourceStatus::Pending(_) | PackageSourceStatus::Processing(_) => {
            Err(package_source_exposures_unavailable(
                "package source exposures are not available until processing completes",
            ))
        }
        PackageSourceStatus::Cancelled(_) => Err(package_source_exposures_unavailable(
            "package source exposures are not available for cancelled package sources",
        )),
        PackageSourceStatus::Failed(_) => Ok(PackageSourceExposuresStatus::Failed),
        PackageSourceStatus::Completed(_) => {
            ensure_package_source_exposure_data_available(db).await?;
            Ok(PackageSourceExposuresStatus::Completed)
        }
        PackageSourceStatus::CompletedWithWarnings(_) => {
            ensure_package_source_exposure_data_available(db).await?;
            Ok(PackageSourceExposuresStatus::CompletedWithWarnings)
        }
    }
}

async fn ensure_package_source_exposure_data_available(
    db: &DatabaseConnection,
) -> Result<(), HttpError> {
    let cve_records_available = has_cve_list_records(db).await.map_err(internal_error)?;
    let active_kev_entry_available = has_active_kev_records(db).await.map_err(internal_error)?;

    if cve_records_available && active_kev_entry_available {
        Ok(())
    } else {
        Err(HttpError::for_unavail(
            Some("VulnerabilityDataUnavailable".to_owned()),
            "vulnerability data is not yet available for package source exposures".to_owned(),
        ))
    }
}

fn package_source_exposures_unavailable(message: &str) -> HttpError {
    HttpError::for_client_error(
        Some("PackageSourceExposuresUnavailable".to_owned()),
        ClientErrorStatusCode::CONFLICT,
        message.to_owned(),
    )
}

fn package_source_exposure_from_versioned_package(
    package: VersionedPackage,
    cve_id: String,
    kev: PackageSourceExposureKevContext,
) -> PackageSourceExposure {
    PackageSourceExposure {
        package,
        cve_id,
        kev,
    }
}

async fn package_source_exposures(
    db: &DatabaseConnection,
    versioned_packages: &[VersionedPackage],
) -> Result<Vec<PackageSourceExposure>, HttpError> {
    let reachable = versioned_packages
        .iter()
        .map(|package| ReachableNpmPackageVersion {
            package_name: package.name.clone(),
            version: package.version.clone(),
            source_evidence: package.purl.clone(),
        })
        .collect::<Vec<_>>();
    let kev_matches = kev_affected_npm_package_versions(db, &reachable)
        .await
        .map_err(internal_error)?;

    Ok(package_source_exposures_from_kev_matches(
        versioned_packages,
        kev_matches,
    ))
}

fn package_source_exposures_from_kev_matches(
    versioned_packages: &[VersionedPackage],
    kev_matches: impl IntoIterator<Item = nv_common::cve::kev::KevAffectedNpmPackageVersion>,
) -> Vec<PackageSourceExposure> {
    let mut seen = HashSet::new();

    kev_matches
        .into_iter()
        .filter_map(|kev_match| {
            let exposure = package_source_exposure_from_kev_match(versioned_packages, kev_match)?;
            let dedup_key = (
                exposure.package.name.clone(),
                exposure.package.version.clone(),
                exposure.cve_id.clone(),
            );

            seen.insert(dedup_key).then_some(exposure)
        })
        .collect()
}

fn package_source_exposure_kev_context(kev: KevContext) -> PackageSourceExposureKevContext {
    PackageSourceExposureKevContext {
        vendor_project: kev.vendor_project,
        product: kev.product,
        vulnerability_name: kev.vulnerability_name,
        date_added: kev.date_added,
    }
}

fn package_source_exposure_from_kev_match(
    versioned_packages: &[VersionedPackage],
    kev_match: nv_common::cve::kev::KevAffectedNpmPackageVersion,
) -> Option<PackageSourceExposure> {
    if kev_match.status != KevNpmMatchStatus::Affected {
        return None;
    }

    let package_name = kev_match.package_name.as_deref()?;
    let affected_version = kev_match.affected_version.as_deref()?;
    let package = versioned_packages
        .iter()
        .find(|package| package.name == package_name && package.version == affected_version)
        .map(|package| VersionedPackage {
            id: package.id,
            name: package.name.clone(),
            version: package.version.clone(),
            ecosystem: package.ecosystem.clone(),
            purl: package.purl.clone(),
            derivations: package.derivations.clone(),
        })?;

    Some(package_source_exposure_from_versioned_package(
        package,
        kev_match.cve_id,
        package_source_exposure_kev_context(kev_match.kev_context),
    ))
}

fn npm_package_name_from_purl(package_url: &str) -> String {
    let encoded_name = package_url
        .strip_prefix("pkg:npm/")
        .and_then(|purl| purl.rsplit_once('@').map(|(name, _)| name))
        .unwrap_or_default();
    percent_decode_str(encoded_name)
        .decode_utf8_lossy()
        .into_owned()
}

async fn cve_ingest_health(
    db: &DatabaseConnection,
    freshness_threshold: Duration,
) -> Result<CveIngestHealth, HttpError> {
    let records_available = has_cve_list_records(db)
        .await
        .map_err(|_| internal_server_error())?;
    let latest_successful_run = latest_successful_cve_list_sync_run(db)
        .await
        .map_err(|_| internal_server_error())?;
    let latest_successful_commit = last_successful_cve_list_sync_commit(db)
        .await
        .map_err(|_| internal_server_error())?
        .map(|commit| commit.as_str().to_owned());
    let latest_run = latest_cve_list_sync_run(db)
        .await
        .map_err(|_| internal_server_error())?
        .map(cve_list_sync_run_health);

    Ok(CveIngestHealth {
        records_available,
        freshness: freshness_from_snapshot(
            records_available,
            latest_successful_run.as_ref().and_then(sync_completed_at),
            freshness_threshold,
            Utc::now(),
        )
        .to_owned(),
        last_successful_sync_at: latest_successful_run.as_ref().and_then(sync_completed_at),
        latest_successful_commit,
        latest_run,
    })
}

async fn kev_ingest_health(
    db: &DatabaseConnection,
    freshness_threshold: Duration,
) -> Result<KevIngestHealth, HttpError> {
    let records_available = has_active_kev_records(db)
        .await
        .map_err(|_| internal_server_error())?;
    let latest_successful_run = latest_successful_kev_sync_run(db)
        .await
        .map_err(|_| internal_server_error())?;
    let latest_run = latest_kev_sync_run(db)
        .await
        .map_err(|_| internal_server_error())?
        .map(kev_sync_run_health);

    Ok(KevIngestHealth {
        records_available,
        freshness: freshness_from_snapshot(
            records_available,
            latest_successful_run.as_ref().and_then(sync_completed_at),
            freshness_threshold,
            Utc::now(),
        )
        .to_owned(),
        last_successful_sync_at: latest_successful_run.as_ref().and_then(sync_completed_at),
        latest_run,
    })
}

async fn data_status(
    db: &DatabaseConnection,
    cve_freshness_threshold: Duration,
    kev_freshness_threshold: Duration,
    now: DateTime<Utc>,
) -> Result<DataStatus, HttpError> {
    let cve_records_available = has_cve_list_records(db)
        .await
        .map_err(|_| internal_server_error())?;
    let cve_latest_successful = latest_successful_cve_list_sync_run(db)
        .await
        .map_err(|_| internal_server_error())?;
    let cve_latest_run = latest_cve_list_sync_run(db)
        .await
        .map_err(|_| internal_server_error())?;
    let kev_records_available = has_active_kev_records(db)
        .await
        .map_err(|_| internal_server_error())?;
    let kev_latest_successful = latest_successful_kev_sync_run(db)
        .await
        .map_err(|_| internal_server_error())?;
    let kev_latest_run = latest_kev_sync_run(db)
        .await
        .map_err(|_| internal_server_error())?;

    Ok(DataStatus {
        cve_list: dataset_data_status(
            cve_records_available,
            cve_latest_successful.as_ref().and_then(sync_completed_at),
            cve_latest_run.as_ref().map(cve_sync_attempt),
            cve_freshness_threshold,
            now,
            "CVE List data affects vulnerability information used by assessments.",
        ),
        cisa_kev: dataset_data_status(
            kev_records_available,
            kev_latest_successful.as_ref().and_then(sync_completed_at),
            kev_latest_run.as_ref().map(kev_sync_attempt),
            kev_freshness_threshold,
            now,
            "CISA KEV data affects whether newly added known exploited vulnerabilities appear in assessments.",
        ),
    })
}

fn dataset_data_status(
    records_available: bool,
    last_successful_sync_at: Option<DateTime<Utc>>,
    latest_attempt: Option<DatasetSyncAttempt>,
    freshness_threshold: Duration,
    now: DateTime<Utc>,
    assessment_impact: &str,
) -> DatasetDataStatus {
    DatasetDataStatus {
        availability: if records_available {
            "available"
        } else {
            "unavailable"
        }
        .to_owned(),
        freshness: freshness_from_snapshot(
            records_available,
            last_successful_sync_at,
            freshness_threshold,
            now,
        )
        .to_owned(),
        last_successful_sync_at,
        latest_attempt,
        assessment_impact: assessment_impact.to_owned(),
    }
}

fn freshness_from_snapshot(
    records_available: bool,
    last_successful_sync_at: Option<DateTime<Utc>>,
    freshness_threshold: Duration,
    now: DateTime<Utc>,
) -> &'static str {
    let Some(last_successful_sync_at) = last_successful_sync_at else {
        return "unknown";
    };
    if !records_available {
        return "unknown";
    }
    match chrono::Duration::from_std(freshness_threshold) {
        Ok(threshold) if now.signed_duration_since(last_successful_sync_at) <= threshold => {
            "current"
        }
        _ => "stale",
    }
}

fn sync_completed_at<T>(run: &T) -> Option<DateTime<Utc>>
where
    T: SyncRunTimestamps,
{
    run.completed_at().map(|value| value.with_timezone(&Utc))
}

trait SyncRunTimestamps {
    fn completed_at(&self) -> Option<sea_orm::prelude::DateTimeWithTimeZone>;
}

impl SyncRunTimestamps for CveListSyncRun {
    fn completed_at(&self) -> Option<sea_orm::prelude::DateTimeWithTimeZone> {
        self.completed_at
    }
}

impl SyncRunTimestamps for KevSyncRun {
    fn completed_at(&self) -> Option<sea_orm::prelude::DateTimeWithTimeZone> {
        self.completed_at
    }
}

fn cve_list_sync_run_health(run: CveListSyncRun) -> CveListSyncRunHealth {
    CveListSyncRunHealth {
        generation: run.generation,
        status: run.status,
        checked_at: run.checked_at.with_timezone(&Utc),
        completed_at: run
            .completed_at
            .map(|completed_at| completed_at.with_timezone(&Utc)),
        records_seen: run.records_seen,
        records_inserted: run.records_inserted,
        records_updated: run.records_updated,
        error: run.error,
    }
}

fn kev_sync_run_health(run: KevSyncRun) -> KevSyncRunHealth {
    KevSyncRunHealth {
        generation: run.generation,
        status: run.status,
        checked_at: run.checked_at.with_timezone(&Utc),
        completed_at: run
            .completed_at
            .map(|completed_at| completed_at.with_timezone(&Utc)),
        records_seen: run.records_seen,
        records_inserted: run.records_inserted,
        records_updated: run.records_updated,
        error: run.error,
    }
}

fn cve_sync_attempt(run: &CveListSyncRun) -> DatasetSyncAttempt {
    DatasetSyncAttempt {
        status: run.status.clone(),
        started_at: run.checked_at.with_timezone(&Utc),
        completed_at: sync_completed_at(run),
        failure_message: failed_sync_message(&run.status),
    }
}

fn kev_sync_attempt(run: &KevSyncRun) -> DatasetSyncAttempt {
    DatasetSyncAttempt {
        status: run.status.clone(),
        started_at: run.checked_at.with_timezone(&Utc),
        completed_at: sync_completed_at(run),
        failure_message: failed_sync_message(&run.status),
    }
}

fn failed_sync_message(status: &str) -> Option<String> {
    (status == "failed").then(|| "The most recent synchronization attempt failed.".to_owned())
}

fn validate_upgrade_assessment_request(body: &PostUpgradeAssessmentBody) -> Result<(), HttpError> {
    validate_upgrade_assessment_package_source(&body.package_source)?;
    if let Some(vulnerable_package) = &body.vulnerable_package {
        validate_upgrade_assessment_vulnerable_package(vulnerable_package)?;
    }
    if request_has_package_specific_refinements(body) && body.vulnerable_package.is_none() {
        return Err(upgrade_validation_error(
            "vulnerablePackage must be provided when cveLinkage, kevLinkage, or candidateVersion is supplied",
        ));
    }
    if let Some(cve_linkage) = &body.cve_linkage {
        if cve_linkage.is_empty() {
            return Err(upgrade_validation_error(
                "cveLinkage must contain at least one CVE identifier when supplied",
            ));
        }
        for cve_id in cve_linkage {
            if !is_cve_id(cve_id) {
                return Err(upgrade_validation_error(
                    "cveLinkage entries must be CVE identifiers",
                ));
            }
        }
    }
    if let Some(kev_linkage) = &body.kev_linkage {
        validate_upgrade_assessment_kev_linkage(kev_linkage)?;
    }
    if body
        .candidate_version
        .as_deref()
        .is_some_and(|version| version.trim().is_empty())
    {
        return Err(upgrade_validation_error(
            "candidateVersion must not be empty when supplied",
        ));
    }
    if body
        .candidate_version
        .as_deref()
        .is_some_and(|version| semver::Version::parse(version).is_err())
    {
        return Err(upgrade_validation_error(
            "candidateVersion must be a valid semantic version when supplied",
        ));
    }
    Ok(())
}

fn validate_upgrade_assessment_package_source(
    package_source: &UpgradeAssessmentPackageSourceInput,
) -> Result<(), HttpError> {
    if package_source.file_name.trim().is_empty() {
        return Err(upgrade_validation_error(
            "packageSource.fileName must not be empty",
        ));
    }
    if package_source.contents.trim().is_empty() {
        return Err(upgrade_validation_error(
            "packageSource.contents must not be empty",
        ));
    }
    NpmPackageJson::parse_package_json(package_source.contents.as_bytes()).map_err(|_| {
        upgrade_validation_error("packageSource.contents must be a valid package source")
    })?;
    Ok(())
}

fn validate_upgrade_assessment_vulnerable_package(
    vulnerable_package: &UpgradeAssessmentVulnerablePackageInput,
) -> Result<(), HttpError> {
    if vulnerable_package.name.trim().is_empty() {
        return Err(upgrade_validation_error(
            "vulnerablePackage.name must not be empty",
        ));
    }
    NpmPackageName::parse(vulnerable_package.name.clone()).map_err(|_| {
        upgrade_validation_error("vulnerablePackage.name must be a valid NPM package name")
    })?;
    if vulnerable_package.version.trim().is_empty() {
        return Err(upgrade_validation_error(
            "vulnerablePackage.version must not be empty",
        ));
    }
    semver::Version::parse(&vulnerable_package.version).map_err(|_| {
        upgrade_validation_error("vulnerablePackage.version must be a valid semantic version")
    })?;
    if let Some(purl) = &vulnerable_package.purl {
        let purl = NpmPackagePurl::parse(purl).map_err(|_| {
            upgrade_validation_error(
                "vulnerablePackage.purl must be a valid NPM PURL when supplied",
            )
        })?;
        if purl.name.as_str() != vulnerable_package.name
            || purl.version != vulnerable_package.version
        {
            return Err(upgrade_validation_error(
                "vulnerablePackage.purl must match vulnerablePackage.name and vulnerablePackage.version",
            ));
        }
    }
    Ok(())
}

fn validate_upgrade_assessment_kev_linkage(
    kev_linkage: &UpgradeAssessmentKevLinkage,
) -> Result<(), HttpError> {
    if kev_linkage.cve_ids.is_empty()
        && kev_linkage.known_exploited.is_none()
        && kev_linkage.references.is_empty()
    {
        return Err(upgrade_validation_error(
            "kevLinkage must include at least one populated field when supplied",
        ));
    }
    for cve_id in &kev_linkage.cve_ids {
        if !is_cve_id(cve_id) {
            return Err(upgrade_validation_error(
                "kevLinkage.cveIds entries must be CVE identifiers",
            ));
        }
    }
    if kev_linkage
        .references
        .iter()
        .any(|reference| reference.trim().is_empty())
    {
        return Err(upgrade_validation_error(
            "kevLinkage.references must not contain empty values",
        ));
    }
    Ok(())
}

fn request_has_package_specific_refinements(body: &PostUpgradeAssessmentBody) -> bool {
    body.cve_linkage.is_some() || body.kev_linkage.is_some() || body.candidate_version.is_some()
}

fn is_cve_id(value: &str) -> bool {
    let Some(number) = value.strip_prefix("CVE-") else {
        return false;
    };
    let Some((year, sequence)) = number.split_once('-') else {
        return false;
    };
    year.len() == 4
        && year.bytes().all(|byte| byte.is_ascii_digit())
        && sequence.len() >= 4
        && sequence.bytes().all(|byte| byte.is_ascii_digit())
}

fn input_from_request(body: &PostUpgradeAssessmentBody) -> UpgradeAssessmentInput {
    body.clone()
}

async fn create_upgrade_assessment(
    db: &DatabaseConnection,
    id: Uuid,
    input: &UpgradeAssessmentInput,
) -> Result<DateTime<Utc>, HttpError> {
    let (package_name, current_version) = stored_assessment_package_fields(input);
    let (trigger_kind, trigger_reference) = stored_assessment_trigger_fields(input);
    let assessment = upgrade_assessments::ActiveModel {
        id: Set(id.to_string()),
        package_name: Set(package_name),
        current_version: Set(current_version),
        trigger_kind: Set(trigger_kind),
        trigger_reference: Set(trigger_reference),
        candidate_version: Set(input.candidate_version.clone()),
        status: Set("pending".to_owned()),
        ..Default::default()
    }
    .insert(db)
    .await
    .map_err(internal_error)?;
    Ok(utc(assessment.created_at))
}

fn stored_assessment_package_fields(input: &UpgradeAssessmentInput) -> (String, String) {
    input.vulnerable_package.as_ref().map_or_else(
        || {
            (
                input.package_source.file_name.clone(),
                "package-source".to_owned(),
            )
        },
        |package| (package.name.clone(), package.version.clone()),
    )
}

fn stored_assessment_trigger_fields(input: &UpgradeAssessmentInput) -> (String, String) {
    if let Some(cve_id) = input.cve_linkage.as_ref().and_then(|ids| ids.first()) {
        return ("cve".to_owned(), cve_id.clone());
    }
    if let Some(cve_id) = input
        .kev_linkage
        .as_ref()
        .and_then(|kev_linkage| kev_linkage.cve_ids.first())
    {
        return ("cve".to_owned(), cve_id.clone());
    }
    (
        "package-source".to_owned(),
        input.package_source.file_name.clone(),
    )
}

async fn complete_upgrade_assessment(
    db: &DatabaseConnection,
    id: Uuid,
    input: UpgradeAssessmentInput,
    base_verdict: UpgradeAssessmentVerdict,
) -> Result<(), HttpError> {
    let report = if matches!(base_verdict, UpgradeAssessmentVerdict::Unknown) {
        initial_assessment_report(input)
    } else {
        match load_hipcheck_run_by_assessment_id(db, &id)
            .await
            .map_err(internal_error)?
        {
            Some(stored) => report_from_hipcheck(input, id, base_verdict, &stored),
            None => initial_assessment_report(input),
        }
    };
    persist_completed_upgrade_report(db, id, report).await
}

async fn complete_discovered_upgrade_assessment(
    db: &DatabaseConnection,
    id: Uuid,
    input: UpgradeAssessmentInput,
    registry_url: url::Url,
    limits: ElaborationLimits,
    max_packument_bytes: usize,
) -> Result<(), HttpError> {
    let candidates = tokio::time::timeout(
        limits.total_run_timeout,
        discover_assessment_candidates(db, &input, registry_url, &limits, max_packument_bytes),
    )
    .await
    .map_err(|_| HttpError::for_internal_error("candidate discovery timed out".to_owned()))??;
    persist_completed_upgrade_report(db, id, discovered_assessment_report(input, candidates)).await
}

async fn persist_completed_upgrade_report(
    db: &DatabaseConnection,
    id: Uuid,
    report: UpgradeAssessmentResult,
) -> Result<(), HttpError> {
    let report = serde_json::to_value(report).map_err(internal_error)?;
    upgrade_assessments::ActiveModel {
        id: Set(id.to_string()),
        status: Set("completed".to_owned()),
        finished_at: Set(Some(Utc::now().fixed_offset())),
        report: Set(Some(report)),
        error: Set(None),
        ..Default::default()
    }
    .update(db)
    .await
    .map_err(internal_error)?;
    Ok(())
}

async fn mark_upgrade_assessment_failed(
    db: &DatabaseConnection,
    id: Uuid,
    error: String,
) -> Result<(), HttpError> {
    upgrade_assessments::ActiveModel {
        id: Set(id.to_string()),
        status: Set("failed".to_owned()),
        finished_at: Set(Some(Utc::now().fixed_offset())),
        error: Set(Some(error)),
        ..Default::default()
    }
    .update(db)
    .await
    .map_err(internal_error)?;
    Ok(())
}

fn assessment_caveat(code: &str, summary: impl Into<String>) -> UpgradeAssessmentCaveat {
    UpgradeAssessmentCaveat {
        code: code.to_owned(),
        summary: summary.into(),
    }
}

fn initial_assessment_report(input: UpgradeAssessmentInput) -> UpgradeAssessmentResult {
    assessment_report(
        input,
        UpgradeAssessmentVerdict::Unknown,
        Vec::new(),
        Vec::new(),
        None,
    )
}

fn discovered_assessment_report(
    input: UpgradeAssessmentInput,
    candidates: Vec<DiscoveredAssessmentCandidate>,
) -> UpgradeAssessmentResult {
    let candidate_versions: Vec<_> = candidates
        .iter()
        .map(discovered_assessment_candidate)
        .collect();
    let verdict = if candidate_versions.iter().any(|candidate| {
        matches!(
            candidate.verdict.as_ref(),
            Some(UpgradeAssessmentVerdict::Recommended)
        )
    }) {
        UpgradeAssessmentVerdict::Recommended
    } else if candidate_versions.iter().any(|candidate| {
        matches!(
            candidate.verdict.as_ref(),
            Some(UpgradeAssessmentVerdict::Caution)
        )
    }) {
        UpgradeAssessmentVerdict::Caution
    } else if candidate_versions.iter().any(|candidate| {
        matches!(
            candidate.verdict.as_ref(),
            Some(UpgradeAssessmentVerdict::Avoid)
        )
    }) {
        UpgradeAssessmentVerdict::Avoid
    } else {
        UpgradeAssessmentVerdict::Unknown
    };
    let mut caveats = vec![
        assessment_caveat(
            "automatic-discovery-ordering",
            "Candidates are ordered by ascending SemVer version. Pre-release and deprecated releases are excluded from automatic discovery.",
        ),
        assessment_caveat(
            "discovery-has-no-supply-chain-analysis",
            "Candidate verdicts use locally ingested KEV matches; supply-chain analysis is only available for an explicitly requested candidate.",
        ),
    ];
    if candidate_versions.is_empty() {
        caveats.push(assessment_caveat(
            "no-discovered-candidates",
            "No applicable newer published NPM candidate was discovered.",
        ));
    }
    let summary = match verdict {
        UpgradeAssessmentVerdict::Recommended => {
            "Night Vision discovered at least one recommended upgrade candidate.".to_owned()
        }
        UpgradeAssessmentVerdict::Caution => {
            "Night Vision discovered upgrade candidates that require review before adoption."
                .to_owned()
        }
        UpgradeAssessmentVerdict::Avoid => {
            "Night Vision did not discover an automatically recommended candidate.".to_owned()
        }
        UpgradeAssessmentVerdict::Unknown => {
            "Night Vision could not determine a discovered-candidate recommendation.".to_owned()
        }
    };
    UpgradeAssessmentResult {
        id: Uuid::nil(),
        status: UpgradeAssessmentWorkflowStatus::Pending,
        input,
        verdict,
        summary,
        findings: Vec::new(),
        evidence: Vec::new(),
        caveats,
        candidate_versions,
        assessed_at: Utc::now(),
    }
}

fn discovered_assessment_candidate(
    discovered: &DiscoveredAssessmentCandidate,
) -> UpgradeAssessmentCandidateVersion {
    let candidate = &discovered.candidate;
    let upgrade_distance = assessment_upgrade_distance(candidate.upgrade_distance);
    let requires_compatibility_review =
        requires_compatibility_review(Some(&upgrade_distance), &candidate.api_compatibility);
    let verdict = if matches!(discovered.base_verdict, UpgradeAssessmentVerdict::Avoid) {
        Some(UpgradeAssessmentVerdict::Avoid)
    } else if requires_compatibility_review {
        Some(UpgradeAssessmentVerdict::Caution)
    } else {
        Some(discovered.base_verdict.clone())
    };
    UpgradeAssessmentCandidateVersion {
        version: candidate.version.to_string(),
        is_requested_candidate: false,
        release_timestamp: None,
        upgrade_distance: Some(upgrade_distance),
        verdict,
    }
}

fn report_from_hipcheck(
    input: UpgradeAssessmentInput,
    assessment_id: Uuid,
    base_verdict: UpgradeAssessmentVerdict,
    stored: &nv_common::hipcheck::storage::StoredHipcheckRun,
) -> UpgradeAssessmentResult {
    let evidence_id = format!("hipcheck-run-{assessment_id}");
    let evidence_link = format!("/assessments/{assessment_id}/evidence");
    let mut findings: Vec<_> = stored
        .findings
        .iter()
        .zip(1_usize..)
        .map(|(finding, finding_number)| UpgradeAssessmentFinding {
            id: format!("hipcheck-finding-{finding_number}"),
            category: UpgradeAssessmentFindingCategory::SupplyChain,
            effect: hipcheck_finding_effect(&finding.effect),
            title: "Hipcheck finding".to_owned(),
            summary: finding.summary.clone(),
            severity: None,
            confidence: None,
            evidence_ids: vec![evidence_id.clone()],
        })
        .collect();
    if stored.run.status != "completed" {
        findings.push(nv_server_api::UpgradeAssessmentFinding {
            id: "hipcheck-incomplete-run".to_owned(),
            category: UpgradeAssessmentFindingCategory::MissingEvidence,
            effect: UpgradeAssessmentFindingEffect::MissingCheck,
            title: "Hipcheck did not complete".to_owned(),
            summary: stored.run.error_message.clone().unwrap_or_else(|| {
                "Hipcheck did not produce complete supply-chain evidence.".to_owned()
            }),
            severity: None,
            confidence: None,
            evidence_ids: vec![evidence_id.clone()],
        });
    }
    assessment_report(
        input,
        base_verdict,
        findings,
        vec![UpgradeAssessmentEvidence {
            id: evidence_id,
            source_type: UpgradeAssessmentEvidenceSourceType::Hipcheck,
            title: "Hipcheck supply-chain analysis".to_owned(),
            summary:
                "Supply-chain findings collected from the Hipcheck run associated with this assessment."
                    .to_owned(),
            url: Some(evidence_link),
            details: None,
            raw_source_identifiers: Vec::new(),
        }],
        stored.run.policy_recommendation.clone(),
    )
}

fn assessment_report(
    input: UpgradeAssessmentInput,
    base_verdict: UpgradeAssessmentVerdict,
    supply_chain_findings: Vec<nv_server_api::UpgradeAssessmentFinding>,
    evidence: Vec<UpgradeAssessmentEvidence>,
    hipcheck_recommendation: Option<String>,
) -> UpgradeAssessmentResult {
    let verdict = upgrade_assessment_verdict(&base_verdict, &supply_chain_findings);
    let current_version = input
        .vulnerable_package
        .as_ref()
        .map(|package| package.version.as_str());
    let candidate_versions = input
        .candidate_version
        .iter()
        .map(|version| {
            let mut candidate = assessment_candidate(current_version, version);
            let requires_compatibility_review = matches!(
                candidate.verdict.as_ref(),
                Some(UpgradeAssessmentVerdict::Caution)
            );
            candidate.verdict = Some(
                if requires_compatibility_review
                    && (matches!(verdict, UpgradeAssessmentVerdict::Recommended)
                        || (matches!(verdict, UpgradeAssessmentVerdict::Unknown)
                            && supply_chain_findings.is_empty()))
                {
                    UpgradeAssessmentVerdict::Caution
                } else {
                    verdict.clone()
                },
            );
            candidate
        })
        .collect();
    let mut caveats = Vec::new();
    if supply_chain_findings.is_empty() {
        caveats.push(assessment_caveat(
            "analysis-pending",
            "Candidate analysis has not yet produced supply-chain findings.",
        ));
    }
    if let Some(recommendation) = hipcheck_recommendation {
        caveats.push(assessment_caveat(
            "hipcheck-policy-recommendation",
            format!(
                "Hipcheck policy recommendation was {recommendation}; Night Vision used the normalized findings above when determining this verdict."
            ),
        ));
    }
    let summary = match verdict {
        UpgradeAssessmentVerdict::Recommended => {
            "Night Vision identified a recommended upgrade candidate.".to_owned()
        }
        UpgradeAssessmentVerdict::Caution => {
            "Night Vision identified an upgrade candidate that requires review before adoption."
                .to_owned()
        }
        UpgradeAssessmentVerdict::Avoid => {
            "Night Vision found blocking issues for the assessed upgrade candidate.".to_owned()
        }
        UpgradeAssessmentVerdict::Unknown => "Not enough evidence for a recommendation.".to_owned(),
    };
    UpgradeAssessmentResult {
        id: Uuid::nil(),
        status: UpgradeAssessmentWorkflowStatus::Pending,
        input,
        verdict,
        summary,
        findings: supply_chain_findings,
        evidence,
        caveats,
        candidate_versions,
        assessed_at: Utc::now(),
    }
}

/// Apply supply-chain evidence without allowing it to weaken a stricter
/// vulnerability or upgrade-domain verdict.
fn upgrade_assessment_verdict(
    base_verdict: &UpgradeAssessmentVerdict,
    findings: &[nv_server_api::UpgradeAssessmentFinding],
) -> UpgradeAssessmentVerdict {
    if matches!(base_verdict, UpgradeAssessmentVerdict::Avoid)
        || findings
            .iter()
            .any(|finding| matches!(finding.effect, UpgradeAssessmentFindingEffect::Blocking))
    {
        return UpgradeAssessmentVerdict::Avoid;
    }
    if matches!(base_verdict, UpgradeAssessmentVerdict::Unknown)
        || findings
            .iter()
            .any(|finding| matches!(finding.effect, UpgradeAssessmentFindingEffect::MissingCheck))
    {
        return UpgradeAssessmentVerdict::Unknown;
    }
    if matches!(base_verdict, UpgradeAssessmentVerdict::Caution)
        || findings
            .iter()
            .any(|finding| matches!(finding.effect, UpgradeAssessmentFindingEffect::Review))
    {
        return UpgradeAssessmentVerdict::Caution;
    }
    UpgradeAssessmentVerdict::Recommended
}

fn hipcheck_finding_effect(effect: &str) -> UpgradeAssessmentFindingEffect {
    match effect {
        "blocking" => UpgradeAssessmentFindingEffect::Blocking,
        "context" => UpgradeAssessmentFindingEffect::Context,
        "missing-check" => UpgradeAssessmentFindingEffect::MissingCheck,
        _ => UpgradeAssessmentFindingEffect::Review,
    }
}

fn assessment_candidate(
    current_version: Option<&str>,
    candidate_version: &str,
) -> UpgradeAssessmentCandidateVersion {
    let (upgrade_distance, requires_compatibility_review) = match current_version {
        Some(current_version) => match (
            semver::Version::parse(current_version),
            semver::Version::parse(candidate_version),
        ) {
            (Ok(current), Ok(candidate)) => {
                let upgrade_distance =
                    assessment_upgrade_distance(upgrade_distance(&current, &candidate));
                let api_compatibility = api_compatibility(&current, &candidate);
                let requires_compatibility_review =
                    requires_compatibility_review(Some(&upgrade_distance), &api_compatibility);
                (Some(upgrade_distance), requires_compatibility_review)
            }
            _ => (Some(UpgradeAssessmentUpgradeDistance::Unknown), true),
        },
        None => (None, false),
    };
    UpgradeAssessmentCandidateVersion {
        version: candidate_version.to_owned(),
        is_requested_candidate: true,
        release_timestamp: None,
        upgrade_distance,
        verdict: Some(if requires_compatibility_review {
            UpgradeAssessmentVerdict::Caution
        } else {
            UpgradeAssessmentVerdict::Unknown
        }),
    }
}

fn requires_compatibility_review(
    upgrade_distance: Option<&UpgradeAssessmentUpgradeDistance>,
    api_compatibility: &ApiCompatibility,
) -> bool {
    matches!(
        upgrade_distance,
        Some(UpgradeAssessmentUpgradeDistance::Major)
    ) || matches!(api_compatibility, ApiCompatibility::NoGuarantee)
}

fn assessment_upgrade_distance(value: UpgradeDistance) -> UpgradeAssessmentUpgradeDistance {
    match value {
        UpgradeDistance::Patch => UpgradeAssessmentUpgradeDistance::Patch,
        UpgradeDistance::Minor => UpgradeAssessmentUpgradeDistance::Minor,
        UpgradeDistance::Major => UpgradeAssessmentUpgradeDistance::Major,
    }
}

fn assessment_status(
    assessment: upgrade_assessments::Model,
) -> Result<UpgradeAssessmentStatus, HttpError> {
    let id = Uuid::parse_str(&assessment.id).map_err(internal_error)?;
    let created_at = utc(assessment.created_at);
    let completed_at = assessment.finished_at.map(utc);
    let status = match assessment.status.as_str() {
        "pending" | "processing" => UpgradeAssessmentWorkflowStatus::Pending,
        "completed" => UpgradeAssessmentWorkflowStatus::Completed,
        "failed" => UpgradeAssessmentWorkflowStatus::Failed,
        _ => return Err(internal_error("upgrade assessment has an invalid status")),
    };
    let error = if matches!(status, UpgradeAssessmentWorkflowStatus::Failed) {
        Some(UpgradeAssessmentError {
            code: None,
            message: assessment
                .error
                .ok_or_else(|| internal_error("failed upgrade assessment has no error"))?,
        })
    } else {
        None
    };
    Ok(UpgradeAssessmentStatus {
        id,
        status: status.clone(),
        created_at,
        updated_at: completed_at.unwrap_or(created_at),
        completed_at: if matches!(status, UpgradeAssessmentWorkflowStatus::Completed) {
            completed_at
        } else {
            None
        },
        error,
    })
}

async fn load_upgrade_assessment(
    db: &DatabaseConnection,
    id: Uuid,
) -> Result<upgrade_assessments::Model, HttpError> {
    upgrade_assessments::Entity::find_by_id(id.to_string())
        .one(db)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| HttpError::for_not_found(None, format!("unknown upgrade assessment {id}")))
}

fn assessment_result(
    assessment: upgrade_assessments::Model,
) -> Result<UpgradeAssessmentResult, HttpError> {
    let id = Uuid::parse_str(&assessment.id).map_err(internal_error)?;
    match assessment.status.as_str() {
        "completed" => {}
        "pending" | "processing" => {
            return Err(upgrade_result_unavailable(
                "upgrade assessment result is not available until processing completes",
            ));
        }
        "failed" => {
            return Err(upgrade_result_unavailable(
                "upgrade assessment did not complete successfully",
            ));
        }
        _ => return Err(internal_error("upgrade assessment has an invalid status")),
    }

    let report = assessment
        .report
        .ok_or_else(|| internal_error("completed upgrade assessment has no report"))?;
    let mut result: UpgradeAssessmentResult =
        serde_json::from_value(report).map_err(internal_error)?;
    result.id = id;
    result.status = UpgradeAssessmentWorkflowStatus::Completed;
    Ok(result)
}

fn upgrade_result_unavailable(message: &str) -> HttpError {
    HttpError::for_client_error(
        Some("UpgradeAssessmentResultUnavailable".to_owned()),
        ClientErrorStatusCode::CONFLICT,
        message.to_owned(),
    )
}

fn utc(value: DateTime<chrono::FixedOffset>) -> DateTime<Utc> {
    value.with_timezone(&Utc)
}

fn internal_error(error: impl std::fmt::Display) -> HttpError {
    HttpError::for_internal_error(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone as _;
    use dropshot::{
        Body, ConfigDropshot,
        test_util::{TestContext, read_json},
    };
    use http::{
        Request, StatusCode,
        header::{AUTHORIZATION, CONTENT_TYPE},
    };
    use httpmock::prelude::*;
    use nv_common::{
        cve::kev::{KevAffectedNpmPackageVersion, KevNpmMatchConfidence},
        db::entities::{
            cisa_kev_entries, cve_list_records, package_source_edges, package_source_versions,
            package_source_warnings, package_sources, package_versions,
        },
        npm::elaboration::lifecycle::FailureKind,
    };
    use sea_orm::{DbBackend, MockDatabase, MockExecResult, Value};
    use slog::{Logger, o};
    use std::time::Duration;

    #[tokio::test]
    async fn startup_reconciliation_runs_before_the_server_accepts_work() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_exec_results([
                MockExecResult {
                    last_insert_id: 0,
                    rows_affected: 2,
                },
                MockExecResult {
                    last_insert_id: 0,
                    rows_affected: 2,
                },
            ])
            .into_connection();

        assert_eq!(reconcile_assessments_on_startup(&db).await.unwrap(), 2);

        let transaction_log = db.into_transaction_log();
        let sql = &transaction_log[0].statements()[0].sql;
        assert!(sql.contains("UPDATE \"hipcheck_runs\""));
        assert!(sql.contains("\"status\" IN"));
    }

    #[test]
    fn checked_in_openapi_matches_the_server_description() {
        let expected: serde_json::Value =
            serde_json::from_str(include_str!("../../openapi/nv-server-openapi.json"))
                .expect("checked-in OpenAPI description is valid JSON");
        let path = std::env::temp_dir().join(format!(
            "night-vision-openapi-{}.json",
            uuid::Uuid::now_v7()
        ));
        let path = Utf8PathBuf::from_path_buf(path).expect("temporary path is UTF-8");
        RestApi::new()
            .expect("API description builds")
            .write_openapi(&path)
            .expect("OpenAPI description writes");
        let actual: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(&path).expect("generated OpenAPI description reads"),
        )
        .expect("generated OpenAPI description is valid JSON");
        std::fs::remove_file(path).expect("temporary OpenAPI description removes");
        assert_eq!(
            actual, expected,
            "run nv-server --openapi to refresh the contract"
        );
    }

    #[tokio::test]
    async fn package_source_rejects_invalid_json_contents() {
        let db = MockDatabase::new(DbBackend::Postgres).into_connection();
        let context = ApiCtx::for_test(db, None);
        let test_context = TestContext::new(
            RestApi::new().expect("API description should build").0,
            context,
            &ConfigDropshot::default(),
            None,
            Logger::root(slog::Discard, o!()),
        );
        let request = Request::builder()
            .method(http::Method::POST)
            .uri(test_context.client_testctx.url("/package-sources"))
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(
                r#"{"fileName":"package.json","contents":"not JSON"}"#,
            ))
            .expect("request should build");

        let error = test_context
            .client_testctx
            .make_request_with_request(request, StatusCode::BAD_REQUEST)
            .await
            .expect_err("invalid package source should be rejected");
        assert_eq!(
            error.error_code.as_deref(),
            Some("InvalidPackageSourceRequest")
        );
        test_context.teardown().await;
    }

    #[tokio::test]
    async fn package_source_cancel_returns_a_visible_cancelled_outcome() {
        let id = Uuid::now_v7();
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_exec_results([MockExecResult {
                last_insert_id: 0,
                rows_affected: 1,
            }])
            .into_connection();
        let context = ApiCtx::for_test(db, None);
        let test_context = TestContext::new(
            RestApi::new().expect("API description should build").0,
            context,
            &ConfigDropshot::default(),
            None,
            Logger::root(slog::Discard, o!()),
        );

        let mut response = test_context
            .client_testctx
            .make_request_no_body(
                http::Method::POST,
                &format!("/package-sources/{id}/cancel"),
                StatusCode::ACCEPTED,
            )
            .await
            .expect("eligible cancellation should be accepted");
        let body: serde_json::Value = read_json(&mut response).await;
        assert_eq!(body, serde_json::json!({ "id": id, "status": "cancelled" }));

        test_context.teardown().await;
    }

    #[tokio::test]
    async fn deleting_package_source_is_hidden_from_reads() {
        let id = Uuid::now_v7();
        let mut deleting = source(id, "deleting", None);
        deleting.cancellation_requested = true;
        deleting.deletion_reason = Some("caller".to_owned());
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![deleting]])
            .into_connection();
        let context = ApiCtx::for_test(db, None);
        let test_context = TestContext::new(
            RestApi::new().expect("API description should build").0,
            context,
            &ConfigDropshot::default(),
            None,
            Logger::root(slog::Discard, o!()),
        );

        test_context
            .client_testctx
            .make_request_no_body(
                http::Method::GET,
                &format!("/package-sources/{id}"),
                StatusCode::NOT_FOUND,
            )
            .await
            .expect_err("deleting resources are not visible");

        test_context.teardown().await;
    }

    #[tokio::test]
    async fn health_routes_serve_public_and_operator_responses() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([
                Vec::<std::collections::BTreeMap<String, Value>>::new(),
                Vec::<std::collections::BTreeMap<String, Value>>::new(),
                Vec::<std::collections::BTreeMap<String, Value>>::new(),
                Vec::<std::collections::BTreeMap<String, Value>>::new(),
                Vec::<std::collections::BTreeMap<String, Value>>::new(),
                Vec::<std::collections::BTreeMap<String, Value>>::new(),
                Vec::<std::collections::BTreeMap<String, Value>>::new(),
            ])
            .into_connection();
        let context = ApiCtx::for_test(db, Some(SecretString::from("operator-token")));
        let test_context = TestContext::new(
            RestApi::new().expect("API description should build").0,
            context,
            &ConfigDropshot::default(),
            None,
            Logger::root(slog::Discard, o!()),
        );

        let mut public_response = test_context
            .client_testctx
            .make_request_no_body(http::Method::GET, "/health", StatusCode::OK)
            .await
            .expect("public health request should succeed");
        let public_body: serde_json::Value = read_json(&mut public_response).await;
        assert_eq!(public_body, serde_json::json!({ "status": "ok" }));

        let request = Request::builder()
            .method(http::Method::GET)
            .uri(test_context.client_testctx.url("/health/diagnostics"))
            .header(AUTHORIZATION, "Bearer operator-token")
            .body(Body::empty())
            .expect("diagnostics request should build");
        let mut diagnostics_response = test_context
            .client_testctx
            .make_request_with_request(request, StatusCode::OK)
            .await
            .expect("authenticated diagnostics request should succeed");
        let diagnostics_body: serde_json::Value = read_json(&mut diagnostics_response).await;
        assert_eq!(
            diagnostics_body,
            serde_json::json!({
                "status": "ok",
                "cveIngest": {
                    "recordsAvailable": false,
                    "freshness": "unknown",
                    "lastSuccessfulSyncAt": null,
                    "latestSuccessfulCommit": null,
                    "latestRun": null,
                },
                "kevIngest": {
                    "recordsAvailable": false,
                    "freshness": "unknown",
                    "lastSuccessfulSyncAt": null,
                    "latestRun": null,
                },
            })
        );

        test_context.teardown().await;
    }

    #[tokio::test]
    async fn data_status_returns_a_bounded_public_summary() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([
                Vec::<std::collections::BTreeMap<String, Value>>::new(),
                Vec::<std::collections::BTreeMap<String, Value>>::new(),
                Vec::<std::collections::BTreeMap<String, Value>>::new(),
                Vec::<std::collections::BTreeMap<String, Value>>::new(),
                Vec::<std::collections::BTreeMap<String, Value>>::new(),
                Vec::<std::collections::BTreeMap<String, Value>>::new(),
            ])
            .into_connection();
        let context = ApiCtx::for_test(db, None);
        let test_context = TestContext::new(
            RestApi::new().expect("API description should build").0,
            context,
            &ConfigDropshot::default(),
            None,
            Logger::root(slog::Discard, o!()),
        );

        let mut response = test_context
            .client_testctx
            .make_request_no_body(http::Method::GET, "/data-status", StatusCode::OK)
            .await
            .expect("data status request should succeed");
        let body: serde_json::Value = read_json(&mut response).await;
        assert_eq!(body["cveList"]["availability"], "unavailable");
        assert_eq!(body["cveList"]["freshness"], "unknown");
        assert_eq!(body["cisaKev"]["availability"], "unavailable");
        assert!(body["cveList"].get("generation").is_none());
        assert!(body["cisaKev"].get("error").is_none());

        test_context.teardown().await;
    }

    #[test]
    fn raw_hipcheck_evidence_requires_an_operator_token() {
        let token = SecretString::from("operator-token");

        assert_eq!(
            raw_hipcheck_evidence_authorization(None, [])
                .unwrap_err()
                .status_code,
            ClientErrorStatusCode::NOT_FOUND
        );
        assert_eq!(
            raw_hipcheck_evidence_authorization(Some(&token), [])
                .unwrap_err()
                .status_code,
            ClientErrorStatusCode::UNAUTHORIZED
        );
        raw_hipcheck_evidence_authorization(Some(&token), [Some("Bearer operator-token")])
            .expect("matching operator token should authorize raw evidence");
    }

    #[test]
    fn health_diagnostics_requires_one_matching_bearer_token() {
        let token = SecretString::from("operator-token");

        assert_eq!(
            health_diagnostics_authorization(None, []),
            HealthDiagnosticsAuthorization::Disabled
        );
        assert_eq!(
            health_diagnostics_authorization(Some(&token), []),
            HealthDiagnosticsAuthorization::Unauthorized
        );
        assert_eq!(
            health_diagnostics_authorization(Some(&token), [Some("Basic operator-token")]),
            HealthDiagnosticsAuthorization::Unauthorized
        );
        assert_eq!(
            health_diagnostics_authorization(Some(&token), [Some("Bearer wrong-token")]),
            HealthDiagnosticsAuthorization::Unauthorized
        );
        assert_eq!(
            health_diagnostics_authorization(
                Some(&token),
                [Some("Bearer operator-token"), Some("Bearer operator-token")],
            ),
            HealthDiagnosticsAuthorization::Unauthorized
        );
        assert_eq!(
            health_diagnostics_authorization(Some(&token), [Some("Bearer operator-token")]),
            HealthDiagnosticsAuthorization::Authorized
        );
    }

    #[test]
    fn dataset_status_distinguishes_initial_current_stale_and_failed_attempts() {
        let now = Utc.with_ymd_and_hms(2026, 9, 22, 12, 0, 0).unwrap();
        let threshold = Duration::from_mins(1);

        let initial = dataset_data_status(false, None, None, threshold, now, "impact");
        assert_eq!(initial.availability, "unavailable");
        assert_eq!(initial.freshness, "unknown");

        let current = dataset_data_status(
            true,
            Some(now - chrono::Duration::seconds(60)),
            None,
            threshold,
            now,
            "impact",
        );
        assert_eq!(current.freshness, "current");

        let failed_attempt = DatasetSyncAttempt {
            status: "failed".to_owned(),
            started_at: now,
            completed_at: Some(now),
            failure_message: failed_sync_message("failed"),
        };
        let stale = dataset_data_status(
            true,
            Some(now - chrono::Duration::seconds(61)),
            Some(failed_attempt),
            threshold,
            now,
            "impact",
        );
        assert_eq!(stale.freshness, "stale");
        assert_eq!(
            stale
                .latest_attempt
                .and_then(|attempt| attempt.failure_message),
            Some("The most recent synchronization attempt failed.".to_owned())
        );
    }

    #[tokio::test]
    async fn upgrade_assessment_checks_capacity_before_candidate_validation() {
        let db = MockDatabase::new(DbBackend::Postgres).into_connection();
        let context = ApiCtx::for_test(db, None);
        let _admission = context
            .try_admit_hipcheck()
            .expect("test must occupy the only Hipcheck slot");
        let test_context = TestContext::new(
            RestApi::new().expect("API description should build").0,
            context,
            &ConfigDropshot::default(),
            None,
            Logger::root(slog::Discard, o!()),
        );
        let request = Request::builder()
            .method(http::Method::POST)
            .uri(test_context.client_testctx.url("/upgrade-assessments"))
            .header(http::header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::json!({
                    "packageSource": {
                        "ecosystem": "npm",
                        "fileName": "package.json",
                        "contents": "{\"name\":\"example-app\",\"version\":\"1.0.0\",\"dependencies\":{\"example-package\":\"1.2.3\"}}"
                    },
                    "vulnerablePackage": {
                        "name": "example-package",
                        "ecosystem": "npm",
                        "version": "1.2.3",
                        "purl": "pkg:npm/example-package@1.2.3"
                    },
                    "cveLinkage": ["CVE-2026-1234"],
                    "candidateVersion": "1.2.4",
                })
                .to_string(),
            ))
            .expect("upgrade request should build");

        let error = test_context
            .client_testctx
            .make_request_with_request(request, StatusCode::SERVICE_UNAVAILABLE)
            .await
            .expect_err("exhausted capacity must reject the request before registry access");

        assert_eq!(error.error_code.as_deref(), Some("AssessmentCapacity"));
        test_context.teardown().await;
    }

    #[tokio::test]
    async fn upgrade_candidate_discovery_checks_capacity_before_persistence() {
        let db = MockDatabase::new(DbBackend::Postgres).into_connection();
        let context = ApiCtx::for_test(db, None);
        let _admission = context
            .try_admit_package_elaboration()
            .expect("test must occupy the only discovery slot");
        let test_context = TestContext::new(
            RestApi::new().expect("API description should build").0,
            context,
            &ConfigDropshot::default(),
            None,
            Logger::root(slog::Discard, o!()),
        );
        let request = Request::builder()
            .method(http::Method::POST)
            .uri(test_context.client_testctx.url("/upgrade-assessments"))
            .header(http::header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::json!({
                    "packageSource": {
                        "ecosystem": "npm",
                        "fileName": "package.json",
                        "contents": "{\"name\":\"example-app\",\"version\":\"1.0.0\",\"dependencies\":{\"example-package\":\"1.2.3\"}}"
                    },
                    "vulnerablePackage": {
                        "name": "example-package",
                        "ecosystem": "npm",
                        "version": "1.2.3",
                        "purl": "pkg:npm/example-package@1.2.3"
                    },
                    "cveLinkage": ["CVE-2026-1234"]
                })
                .to_string(),
            ))
            .expect("upgrade request should build");

        let error = test_context
            .client_testctx
            .make_request_with_request(request, StatusCode::SERVICE_UNAVAILABLE)
            .await
            .expect_err("exhausted discovery capacity must reject before persistence");

        assert_eq!(
            error.error_code.as_deref(),
            Some("CandidateDiscoveryCapacity")
        );
        test_context.teardown().await;
    }

    #[tokio::test]
    async fn candidate_discovery_enforces_total_run_timeout() {
        let server = MockServer::start();
        let _registry = server.mock(|when, then| {
            when.method(GET).path("/example-package");
            then.status(200).delay(Duration::from_millis(50));
        });
        let db = MockDatabase::new(DbBackend::Postgres).into_connection();
        let limits = ElaborationLimits {
            request_timeout: Duration::from_secs(1),
            total_run_timeout: Duration::from_millis(1),
            ..ElaborationLimits::default()
        };

        let error = complete_discovered_upgrade_assessment(
            &db,
            Uuid::now_v7(),
            UpgradeAssessmentInput {
                candidate_version: None,
                ..input()
            },
            url::Url::parse(&format!("{}/", server.base_url())).expect("valid mock URL"),
            limits,
            1024,
        )
        .await
        .expect_err("total-run timeout must cancel candidate discovery");

        assert_eq!(error.internal_message, "candidate discovery timed out");
    }

    #[test]
    fn unauthorized_diagnostics_response_has_bearer_challenge() {
        let error = health_diagnostics_unauthorized();

        assert_eq!(error.status_code, ClientErrorStatusCode::UNAUTHORIZED);
        assert_eq!(
            error.error_code.as_deref(),
            Some("HealthDiagnosticsUnauthorized")
        );
        assert_eq!(
            error
                .headers
                .as_ref()
                .and_then(|headers| headers.get(WWW_AUTHENTICATE)),
            Some(&HeaderValue::from_static("Bearer"))
        );
    }

    #[test]
    fn unauthorized_raw_evidence_response_has_bearer_challenge() {
        let error = raw_hipcheck_evidence_unauthorized();

        assert_eq!(error.status_code, ClientErrorStatusCode::UNAUTHORIZED);
        assert_eq!(
            error.error_code.as_deref(),
            Some("RawHipcheckEvidenceUnauthorized")
        );
        assert_eq!(
            error
                .headers
                .as_ref()
                .and_then(|headers| headers.get(WWW_AUTHENTICATE)),
            Some(&HeaderValue::from_static("Bearer"))
        );
    }

    fn hipcheck_run() -> hipcheck_runs::Model {
        hipcheck_runs::Model {
            id: 7,
            assessment_id: "0198f30e-2bfa-7000-8000-000000000007".to_owned(),
            package_version_id: 1,
            affected_purl: None,
            status: "failed".to_owned(),
            raw_json: None,
            raw_json_bytes: 0,
            raw_json_truncated: false,
            stdout: Some("partial report".to_owned()),
            stdout_truncated: true,
            stderr: Some("plugin warning".to_owned()),
            stderr_truncated: false,
            exit_status: Some(1),
            error_kind: Some("target-resolution".to_owned()),
            error_message: Some("package version has no usable source repository".to_owned()),
            retryable: Some(false),
            schema_version: None,
            hipcheck_version: None,
            hipcheck_commit: None,
            target_kind: None,
            target_purl: None,
            source_repository_url: Some("https://github.com/example/project".to_owned()),
            policy_id: None,
            policy_version: None,
            policy_source: None,
            policy_recommendation: None,
            created_at: Utc::now().fixed_offset(),
        }
    }

    #[test]
    fn assessment_diagnostics_expose_persisted_run_fields() {
        let diagnostics = assessment_diagnostics(&hipcheck_run());

        assert_eq!(diagnostics.stdout.as_deref(), Some("partial report"));
        assert!(diagnostics.stdout_truncated);
        assert_eq!(diagnostics.stderr.as_deref(), Some("plugin warning"));
        assert_eq!(diagnostics.exit_status, Some(1));
        assert_eq!(diagnostics.error_kind.as_deref(), Some("target-resolution"));
        assert_eq!(
            diagnostics.error_message.as_deref(),
            Some("package version has no usable source repository")
        );
        assert_eq!(diagnostics.retryable, Some(false));
    }

    #[test]
    fn source_repository_url_falls_back_when_version_repository_is_unusable() {
        let version_repository = nv_common::npm::packument::Repository {
            type_field: "git".to_owned(),
            url: "git://gitlab.example/project.git".to_owned(),
        };
        let package_repository = nv_common::npm::packument::Repository {
            type_field: "git".to_owned(),
            url: "https://github.com/example/project.git".to_owned(),
        };

        assert_eq!(
            source_repository_url(Some(&version_repository), Some(&package_repository)),
            Some("https://github.com/example/project.git".to_owned())
        );
    }

    #[test]
    fn repository_for_candidate_uses_full_packument_metadata_when_compact_is_missing_it() {
        let compact = nv_common::npm::packument::parse_packument(
            serde_json::json!({
                "name": "systeminformation",
                "dist-tags": { "latest": "5.3.1" },
                "versions": {
                    "5.3.1": {
                        "name": "systeminformation",
                        "version": "5.3.1",
                        "dist": {
                            "tarball": "https://registry.example/systeminformation-5.3.1.tgz",
                            "shasum": "0123456789012345678901234567890123456789"
                        }
                    }
                }
            })
            .to_string()
            .as_bytes(),
        )
        .expect("compact fixture parses");
        let full = nv_common::npm::packument::parse_packument(
            serde_json::json!({
                "name": "systeminformation",
                "repository": {
                    "type": "git",
                    "url": "git+https://github.com/sebhildebrandt/systeminformation.git"
                },
                "dist-tags": { "latest": "5.3.1" },
                "versions": {
                    "5.3.1": {
                        "name": "systeminformation",
                        "version": "5.3.1",
                        "dist": {
                            "tarball": "https://registry.example/systeminformation-5.3.1.tgz",
                            "shasum": "0123456789012345678901234567890123456789"
                        }
                    }
                }
            })
            .to_string()
            .as_bytes(),
        )
        .expect("full fixture parses");
        let candidate =
            validate_explicit_candidate(&compact, "systeminformation", "5.3.0", "5.3.1")
                .expect("candidate validates");

        assert_eq!(repository_for_candidate(&compact, &candidate), None);
        assert_eq!(
            repository_for_candidate(&full, &candidate).as_deref(),
            Some("https://github.com/sebhildebrandt/systeminformation.git")
        );
    }

    #[tokio::test]
    async fn repository_for_assessment_target_rejects_full_packument_fetch_failure() {
        let server = MockServer::start();
        let compact = packument_with_versions(&["5.3.1"], None);
        let compact_mock = server.mock(|when, then| {
            when.method(GET)
                .path("/systeminformation")
                .header("accept", "application/vnd.npm.install-v1+json");
            then.status(200).body(compact.to_string());
        });
        let full_mock = server.mock(|when, then| {
            when.method(GET)
                .path("/systeminformation")
                .header_not("accept", "application/vnd.npm.install-v1+json");
            then.status(503);
        });
        let client = NpmRegistryClient::new(
            url::Url::parse(&format!("{}/", server.base_url())).expect("valid mock URL"),
            1024,
            std::time::Duration::from_secs(1),
        )
        .expect("valid client");
        let package = NpmPackageName::parse("systeminformation".to_owned()).expect("valid name");
        let compact = client
            .fetch(&package)
            .await
            .expect("compact fetch succeeds");
        let candidate =
            validate_explicit_candidate(&compact, "systeminformation", "5.3.0", "5.3.1")
                .expect("candidate validates");

        let error = repository_for_assessment_target(
            &client, &package, "5.3.0", "5.3.1", &compact, &candidate,
        )
        .await
        .expect_err("full packument fetch failure rejects the assessment before queueing");

        assert_eq!(
            error.error_code.as_deref(),
            Some("InvalidUpgradeAssessment")
        );
        compact_mock.assert();
        full_mock.assert();
    }

    #[tokio::test]
    async fn repository_for_assessment_target_uses_full_packument_repository_metadata() {
        let server = MockServer::start();
        let compact = packument_with_versions(&["5.3.1"], None);
        let full = packument_with_versions(
            &["5.3.1"],
            Some(serde_json::json!({
                "type": "git",
                "url": "git+https://github.com/sebhildebrandt/systeminformation.git"
            })),
        );
        let compact_mock = server.mock(|when, then| {
            when.method(GET)
                .path("/systeminformation")
                .header("accept", "application/vnd.npm.install-v1+json");
            then.status(200).body(compact.to_string());
        });
        let full_mock = server.mock(|when, then| {
            when.method(GET)
                .path("/systeminformation")
                .header_not("accept", "application/vnd.npm.install-v1+json");
            then.status(200).body(full.to_string());
        });
        let client = NpmRegistryClient::new(
            url::Url::parse(&format!("{}/", server.base_url())).expect("valid mock URL"),
            1024,
            std::time::Duration::from_secs(1),
        )
        .expect("valid client");
        let package = NpmPackageName::parse("systeminformation".to_owned()).expect("valid name");
        let compact = client
            .fetch(&package)
            .await
            .expect("compact fetch succeeds");
        let candidate =
            validate_explicit_candidate(&compact, "systeminformation", "5.3.0", "5.3.1")
                .expect("candidate validates");

        let repository = repository_for_assessment_target(
            &client, &package, "5.3.0", "5.3.1", &compact, &candidate,
        )
        .await
        .expect("full packument metadata resolves the target repository");

        assert_eq!(
            repository.as_deref(),
            Some("https://github.com/sebhildebrandt/systeminformation.git")
        );
        compact_mock.assert();
        full_mock.assert();
    }

    #[tokio::test]
    async fn repository_for_assessment_target_rejects_missing_target_in_full_packument() {
        let server = MockServer::start();
        let compact = packument_with_versions(&["5.3.1"], None);
        let full = packument_with_versions(
            &["5.3.2"],
            Some(serde_json::json!({
                "type": "git",
                "url": "git+https://github.com/sebhildebrandt/systeminformation.git"
            })),
        );
        let compact_mock = server.mock(|when, then| {
            when.method(GET)
                .path("/systeminformation")
                .header("accept", "application/vnd.npm.install-v1+json");
            then.status(200).body(compact.to_string());
        });
        let full_mock = server.mock(|when, then| {
            when.method(GET)
                .path("/systeminformation")
                .header_not("accept", "application/vnd.npm.install-v1+json");
            then.status(200).body(full.to_string());
        });
        let client = NpmRegistryClient::new(
            url::Url::parse(&format!("{}/", server.base_url())).expect("valid mock URL"),
            1024,
            std::time::Duration::from_secs(1),
        )
        .expect("valid client");
        let package = NpmPackageName::parse("systeminformation".to_owned()).expect("valid name");
        let compact = client
            .fetch(&package)
            .await
            .expect("compact fetch succeeds");
        let candidate =
            validate_explicit_candidate(&compact, "systeminformation", "5.3.0", "5.3.1")
                .expect("candidate validates");

        let error = repository_for_assessment_target(
            &client, &package, "5.3.0", "5.3.1", &compact, &candidate,
        )
        .await
        .expect_err("missing full-packument target rejects the assessment before queueing");

        assert_eq!(
            error.error_code.as_deref(),
            Some("InvalidUpgradeAssessment")
        );
        compact_mock.assert();
        full_mock.assert();
    }

    fn packument_with_versions(
        versions: &[&str],
        repository: Option<serde_json::Value>,
    ) -> serde_json::Value {
        let versions = versions
            .iter()
            .map(|version| {
                (
                    (*version).to_owned(),
                    serde_json::json!({
                        "name": "systeminformation",
                        "version": version,
                        "dist": {
                            "tarball": format!("https://registry.example/systeminformation-{version}.tgz"),
                            "shasum": "0123456789012345678901234567890123456789"
                        }
                    }),
                )
            })
            .collect::<serde_json::Map<_, _>>();
        let mut packument = serde_json::json!({
            "name": "systeminformation",
            "dist-tags": { "latest": versions.keys().next_back() },
            "versions": versions,
        });
        if let Some(repository) = repository {
            packument
                .as_object_mut()
                .expect("packument is an object")
                .insert("repository".to_owned(), repository);
        }
        packument
    }

    #[test]
    fn package_source_exposure_from_kev_match_maps_affected_match_to_api_exposure() {
        let package = VersionedPackage {
            id: Uuid::from_u64_pair(11, 22),
            name: "systeminformation".to_owned(),
            version: "5.3.1".to_owned(),
            ecosystem: PackageSourceEcosystem::Npm,
            purl: "pkg:npm/systeminformation@5.3.1".to_owned(),
            derivations: vec![vec!["systeminformation".to_owned()]],
        };
        let kev_match = KevAffectedNpmPackageVersion {
            cve_id: "CVE-2024-1234".to_owned(),
            kev_context: KevContext {
                cve_id: "CVE-2024-1234".to_owned(),
                vendor_project: Some("Acme".to_owned()),
                product: Some("systeminformation".to_owned()),
                vulnerability_name: Some("Remote code execution".to_owned()),
                date_added: Some("2024-02-03".to_owned()),
            },
            package_name: Some("systeminformation".to_owned()),
            affected_version: Some("5.3.1".to_owned()),
            source_evidence: vec!["dependencies > systeminformation@5.3.1".to_owned()],
            confidence: KevNpmMatchConfidence::High,
            caveats: Vec::new(),
            status: KevNpmMatchStatus::Affected,
        };

        let exposure =
            package_source_exposure_from_kev_match(std::slice::from_ref(&package), kev_match)
                .expect("affected KEV match should map to an API exposure");

        assert_eq!(exposure.package.id, package.id);
        assert_eq!(exposure.package.name, package.name);
        assert_eq!(exposure.package.version, package.version);
        assert_eq!(exposure.package.purl, package.purl);
        assert_eq!(exposure.package.derivations, package.derivations);
        assert_eq!(
            serde_json::to_value(&exposure.package.ecosystem).expect("ecosystem serializes"),
            serde_json::to_value(&package.ecosystem).expect("ecosystem serializes")
        );
        assert_eq!(exposure.cve_id, "CVE-2024-1234");
        assert_eq!(exposure.kev.vendor_project.as_deref(), Some("Acme"));
        assert_eq!(exposure.kev.product.as_deref(), Some("systeminformation"));
        assert_eq!(
            exposure.kev.vulnerability_name.as_deref(),
            Some("Remote code execution")
        );
        assert_eq!(exposure.kev.date_added.as_deref(), Some("2024-02-03"));
    }

    #[test]
    fn package_source_exposures_from_kev_matches_deduplicates_same_package_version_and_cve() {
        let package = VersionedPackage {
            id: Uuid::new_v4(),
            name: "left-pad".to_owned(),
            version: "1.2.3".to_owned(),
            ecosystem: PackageSourceEcosystem::Npm,
            purl: "pkg:npm/left-pad@1.2.3".to_owned(),
            derivations: vec![vec!["<root>".to_owned(), "left-pad".to_owned()]],
        };
        let kev_context = KevContext {
            cve_id: "CVE-2026-1234".to_owned(),
            vendor_project: Some("Example Vendor".to_owned()),
            product: Some("left-pad".to_owned()),
            vulnerability_name: Some("Example vulnerability".to_owned()),
            date_added: Some("2026-01-15".to_owned()),
        };
        let kev_matches = vec![
            nv_common::cve::kev::KevAffectedNpmPackageVersion {
                package_name: Some("left-pad".to_owned()),
                affected_version: Some("1.2.3".to_owned()),
                cve_id: "CVE-2026-1234".to_owned(),
                source_evidence: vec!["dependencies > left-pad@1.2.3".to_owned()],
                confidence: KevNpmMatchConfidence::High,
                caveats: Vec::new(),
                status: KevNpmMatchStatus::Affected,
                kev_context: kev_context.clone(),
            },
            nv_common::cve::kev::KevAffectedNpmPackageVersion {
                package_name: Some("left-pad".to_owned()),
                affected_version: Some("1.2.3".to_owned()),
                cve_id: "CVE-2026-1234".to_owned(),
                source_evidence: vec!["dependencies > left-pad@1.2.3".to_owned()],
                confidence: KevNpmMatchConfidence::High,
                caveats: Vec::new(),
                status: KevNpmMatchStatus::Affected,
                kev_context,
            },
        ];

        let exposures =
            package_source_exposures_from_kev_matches(std::slice::from_ref(&package), kev_matches);

        assert_eq!(exposures.len(), 1);
        assert_eq!(exposures[0].package.id, package.id);
        assert_eq!(exposures[0].package.name, package.name);
        assert_eq!(exposures[0].package.version, package.version);
        assert_eq!(
            serde_json::to_value(&exposures[0].package.ecosystem).expect("ecosystem serializes"),
            serde_json::to_value(&package.ecosystem).expect("ecosystem serializes")
        );
        assert_eq!(exposures[0].package.purl, package.purl);
        assert_eq!(exposures[0].package.derivations, package.derivations);
        assert_eq!(exposures[0].cve_id, "CVE-2026-1234");
    }

    #[test]
    fn package_source_exposures_from_kev_matches_ignores_unaffected_or_unmatched_rows() {
        let package = VersionedPackage {
            id: Uuid::new_v4(),
            name: "left-pad".to_owned(),
            version: "1.2.3".to_owned(),
            ecosystem: PackageSourceEcosystem::Npm,
            purl: "pkg:npm/left-pad@1.2.3".to_owned(),
            derivations: vec![vec!["<root>".to_owned(), "left-pad".to_owned()]],
        };
        let kept_kev_context = KevContext {
            cve_id: "CVE-2026-1234".to_owned(),
            vendor_project: Some("Example Vendor".to_owned()),
            product: Some("left-pad".to_owned()),
            vulnerability_name: Some("Reachable vulnerability".to_owned()),
            date_added: Some("2026-01-15".to_owned()),
        };
        let ignored_kev_context = KevContext {
            cve_id: "CVE-2026-9999".to_owned(),
            vendor_project: Some("Other Vendor".to_owned()),
            product: Some("other-package".to_owned()),
            vulnerability_name: Some("Ignored vulnerability".to_owned()),
            date_added: Some("2026-02-01".to_owned()),
        };
        let kev_matches = vec![
            nv_common::cve::kev::KevAffectedNpmPackageVersion {
                package_name: Some("left-pad".to_owned()),
                affected_version: Some("1.2.3".to_owned()),
                cve_id: "CVE-2026-1234".to_owned(),
                source_evidence: vec!["dependencies > left-pad@1.2.3".to_owned()],
                confidence: KevNpmMatchConfidence::High,
                caveats: Vec::new(),
                status: KevNpmMatchStatus::Affected,
                kev_context: kept_kev_context,
            },
            nv_common::cve::kev::KevAffectedNpmPackageVersion {
                package_name: Some("left-pad".to_owned()),
                affected_version: Some("1.2.3".to_owned()),
                cve_id: "CVE-2026-9999".to_owned(),
                source_evidence: vec!["dependencies > left-pad@1.2.3".to_owned()],
                confidence: KevNpmMatchConfidence::Unknown,
                caveats: vec!["package metadata was ambiguous".to_owned()],
                status: KevNpmMatchStatus::Unknown,
                kev_context: ignored_kev_context.clone(),
            },
            nv_common::cve::kev::KevAffectedNpmPackageVersion {
                package_name: Some("other-package".to_owned()),
                affected_version: Some("9.9.9".to_owned()),
                cve_id: "CVE-2026-0001".to_owned(),
                source_evidence: vec!["dependencies > other-package@9.9.9".to_owned()],
                confidence: KevNpmMatchConfidence::High,
                caveats: Vec::new(),
                status: KevNpmMatchStatus::Affected,
                kev_context: ignored_kev_context,
            },
        ];

        let exposures =
            package_source_exposures_from_kev_matches(std::slice::from_ref(&package), kev_matches);

        assert_eq!(exposures.len(), 1);
        assert_eq!(exposures[0].package.id, package.id);
        assert_eq!(exposures[0].package.name, package.name);
        assert_eq!(exposures[0].package.version, package.version);
        assert_eq!(
            serde_json::to_value(&exposures[0].package.ecosystem).expect("ecosystem serializes"),
            serde_json::to_value(&package.ecosystem).expect("ecosystem serializes")
        );
        assert_eq!(exposures[0].package.purl, package.purl);
        assert_eq!(exposures[0].package.derivations, package.derivations);
        assert_eq!(exposures[0].cve_id, "CVE-2026-1234");
        assert_eq!(
            exposures[0].kev.vulnerability_name.as_deref(),
            Some("Reachable vulnerability")
        );
    }

    #[tokio::test]
    async fn get_package_source_exposures_returns_not_found_for_unknown_source() {
        let id = Uuid::now_v7();
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([Vec::<package_sources::Model>::new()])
            .into_connection();
        let context = ApiCtx::for_test(db, None);
        let test_context = TestContext::new(
            RestApi::new().expect("API description should build").0,
            context,
            &ConfigDropshot::default(),
            None,
            Logger::root(slog::Discard, o!()),
        );

        let error = test_context
            .client_testctx
            .make_request_no_body(
                http::Method::GET,
                &format!("/package-sources/{id}/exposures"),
                StatusCode::NOT_FOUND,
            )
            .await
            .expect_err("unknown package source exposures should return 404");

        assert_eq!(error.error_code, None);
        assert_eq!(error.message, "Not Found");
        test_context.teardown().await;
    }

    #[tokio::test]
    async fn get_package_source_exposures_returns_conflict_while_processing() {
        let id = Uuid::now_v7();
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![source(id, "pending", None)]])
            .into_connection();
        let context = ApiCtx::for_test(db, None);
        let test_context = TestContext::new(
            RestApi::new().expect("API description should build").0,
            context,
            &ConfigDropshot::default(),
            None,
            Logger::root(slog::Discard, o!()),
        );

        let error = test_context
            .client_testctx
            .make_request_no_body(
                http::Method::GET,
                &format!("/package-sources/{id}/exposures"),
                StatusCode::CONFLICT,
            )
            .await
            .expect_err("processing package source exposures should return 409");

        assert_eq!(
            error.error_code.as_deref(),
            Some("PackageSourceExposuresUnavailable")
        );
        test_context.teardown().await;
    }

    #[tokio::test]
    async fn get_package_source_exposures_returns_completed_with_exposures() {
        let id = Uuid::now_v7();
        let now = Utc::now().fixed_offset();
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![source(id, "completed", None)]])
            .append_query_results([vec![package_source_versions::Model {
                id: 1,
                source_id: 1,
                package_version_id: 1,
            }]])
            .append_query_results([vec![package_versions::Model {
                id: 1,
                package_id: 1,
                version: "1.2.3".to_owned(),
                package_url: "pkg:npm/left-pad@1.2.3".to_owned(),
                source_repository: None,
                source_repository_tag: None,
            }]])
            .append_query_results([vec![package_source_edges::Model {
                id: 1,
                source_id: 1,
                parent_package_version_id: None,
                child_package_version_id: 1,
                root_dependency_kind: Some("dependencies".to_owned()),
                declared_dependency: Some("left-pad".to_owned()),
                declared_specification: Some("1.2.3".to_owned()),
            }]])
            .append_query_results([Vec::<package_source_warnings::Model>::new()])
            .append_query_results([vec![cve_list_records::Model {
                cve_id: "CVE-2026-1234".to_owned(),
                record_format_version: "5.1".to_owned(),
                record: serde_json::json!({
                    "containers": {
                        "cna": {
                            "affected": [{
                                "vendor": "Example Vendor",
                                "product": "left-pad",
                                "packageName": "left-pad",
                                "collectionURL": "https://registry.npmjs.org",
                                "versions": [{
                                    "version": "1.2.3",
                                    "status": "affected"
                                }]
                            }]
                        }
                    }
                }),
                deleted: false,
                first_seen_at: now,
                last_seen_at: now,
                updated_at: now,
            }]])
            .append_query_results([vec![std::collections::BTreeMap::from([(
                "cve_id".to_owned(),
                "CVE-2026-1234".into(),
            )])]])
            .append_query_results([vec![cisa_kev_entries::Model {
                cve_id: "CVE-2026-1234".to_owned(),
                entry: serde_json::json!({
                    "cveID": "CVE-2026-1234",
                    "vendorProject": "Example Vendor",
                    "product": "left-pad",
                    "vulnerabilityName": "Example vulnerability",
                    "dateAdded": "2026-08-03"
                }),
                removed_at: None,
                first_seen_at: now,
                last_seen_at: now,
                updated_at: now,
            }]])
            .append_query_results([vec![cve_list_records::Model {
                cve_id: "CVE-2026-1234".to_owned(),
                record_format_version: "5.1".to_owned(),
                record: serde_json::json!({
                    "containers": {
                        "cna": {
                            "affected": [{
                                "vendor": "Example Vendor",
                                "product": "left-pad",
                                "packageName": "left-pad",
                                "collectionURL": "https://registry.npmjs.org",
                                "versions": [{
                                    "version": "1.2.3",
                                    "status": "affected"
                                }]
                            }]
                        }
                    }
                }),
                deleted: false,
                first_seen_at: now,
                last_seen_at: now,
                updated_at: now,
            }]])
            .into_connection();
        let context = ApiCtx::for_test(db, None);
        let test_context = TestContext::new(
            RestApi::new().expect("API description should build").0,
            context,
            &ConfigDropshot::default(),
            None,
            Logger::root(slog::Discard, o!()),
        );

        let mut response = test_context
            .client_testctx
            .make_request_no_body(
                http::Method::GET,
                &format!("/package-sources/{id}/exposures"),
                StatusCode::OK,
            )
            .await
            .expect("completed package source exposures should return 200");

        let body: serde_json::Value = read_json(&mut response).await;

        assert_eq!(
            body,
            serde_json::json!({
                "id": id,
                "status": "completed",
                "exposures": [{
                    "package": {
                        "id": Uuid::from_u64_pair(1, 1),
                        "name": "left-pad",
                        "version": "1.2.3",
                        "ecosystem": "npm",
                        "purl": "pkg:npm/left-pad@1.2.3",
                        "derivations": [[
                            "<root>",
                            "pkg:npm/left-pad@1.2.3"
                        ]]
                    },
                    "cveId": "CVE-2026-1234",
                    "kev": {
                        "vendorProject": "Example Vendor",
                        "product": "left-pad",
                        "vulnerabilityName": "Example vulnerability",
                        "dateAdded": "2026-08-03"
                    }
                }]
            })
        );

        test_context.teardown().await;
    }

    #[tokio::test]
    async fn get_package_source_exposures_returns_completed_with_no_exposures() {
        let id = Uuid::now_v7();
        let now = Utc::now().fixed_offset();
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![source(id, "completed", None)]])
            .append_query_results([vec![package_source_versions::Model {
                id: 1,
                source_id: 1,
                package_version_id: 1,
            }]])
            .append_query_results([vec![package_versions::Model {
                id: 1,
                package_id: 1,
                version: "1.2.3".to_owned(),
                package_url: "pkg:npm/left-pad@1.2.3".to_owned(),
                source_repository: None,
                source_repository_tag: None,
            }]])
            .append_query_results([vec![package_source_edges::Model {
                id: 1,
                source_id: 1,
                parent_package_version_id: None,
                child_package_version_id: 1,
                root_dependency_kind: Some("dependencies".to_owned()),
                declared_dependency: Some("left-pad".to_owned()),
                declared_specification: Some("1.2.3".to_owned()),
            }]])
            .append_query_results([Vec::<package_source_warnings::Model>::new()])
            .append_query_results([vec![cve_list_records::Model {
                cve_id: "CVE-2026-9999".to_owned(),
                record_format_version: "5.1".to_owned(),
                record: serde_json::json!({
                    "containers": {
                        "cna": {
                            "affected": [{
                                "vendor": "Example Vendor",
                                "product": "different-package",
                                "packageName": "different-package",
                                "collectionURL": "https://registry.npmjs.org",
                                "versions": [{
                                    "version": "9.9.9",
                                    "status": "affected"
                                }]
                            }]
                        }
                    }
                }),
                deleted: false,
                first_seen_at: now,
                last_seen_at: now,
                updated_at: now,
            }]])
            .append_query_results([vec![std::collections::BTreeMap::from([(
                "cve_id".to_owned(),
                "CVE-2026-9999".into(),
            )])]])
            .append_query_results([vec![cisa_kev_entries::Model {
                cve_id: "CVE-2026-9999".to_owned(),
                entry: serde_json::json!({
                    "cveID": "CVE-2026-9999",
                    "vendorProject": "Example Vendor",
                    "product": "different-package",
                    "vulnerabilityName": "Unrelated vulnerability",
                    "dateAdded": "2026-08-03"
                }),
                removed_at: None,
                first_seen_at: now,
                last_seen_at: now,
                updated_at: now,
            }]])
            .append_query_results([vec![cve_list_records::Model {
                cve_id: "CVE-2026-9999".to_owned(),
                record_format_version: "5.1".to_owned(),
                record: serde_json::json!({
                    "containers": {
                        "cna": {
                            "affected": [{
                                "vendor": "Example Vendor",
                                "product": "different-package",
                                "packageName": "different-package",
                                "collectionURL": "https://registry.npmjs.org",
                                "versions": [{
                                    "version": "9.9.9",
                                    "status": "affected"
                                }]
                            }]
                        }
                    }
                }),
                deleted: false,
                first_seen_at: now,
                last_seen_at: now,
                updated_at: now,
            }]])
            .into_connection();
        let context = ApiCtx::for_test(db, None);
        let test_context = TestContext::new(
            RestApi::new().expect("API description should build").0,
            context,
            &ConfigDropshot::default(),
            None,
            Logger::root(slog::Discard, o!()),
        );

        let mut response = test_context
            .client_testctx
            .make_request_no_body(
                http::Method::GET,
                &format!("/package-sources/{id}/exposures"),
                StatusCode::OK,
            )
            .await
            .expect("completed package source exposures with no matches should return 200");

        let body: serde_json::Value = read_json(&mut response).await;

        assert_eq!(
            body,
            serde_json::json!({
                "id": id,
                "status": "completed",
                "exposures": []
            })
        );

        test_context.teardown().await;
    }

    #[tokio::test]
    async fn get_package_source_exposures_when_vulnerability_data_is_incomplete() {
        let id = Uuid::now_v7();
        let now = Utc::now().fixed_offset();
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![source(id, "completed", None)]])
            .append_query_results([vec![package_source_versions::Model {
                id: 1,
                source_id: 1,
                package_version_id: 1,
            }]])
            .append_query_results([vec![package_versions::Model {
                id: 1,
                package_id: 1,
                version: "1.2.3".to_owned(),
                package_url: "pkg:npm/left-pad@1.2.3".to_owned(),
                source_repository: None,
                source_repository_tag: None,
            }]])
            .append_query_results([vec![package_source_edges::Model {
                id: 1,
                source_id: 1,
                parent_package_version_id: None,
                child_package_version_id: 1,
                root_dependency_kind: Some("dependencies".to_owned()),
                declared_dependency: Some("left-pad".to_owned()),
                declared_specification: Some("1.2.3".to_owned()),
            }]])
            .append_query_results([Vec::<package_source_warnings::Model>::new()])
            .append_query_results([vec![cve_list_records::Model {
                cve_id: "CVE-2026-1234".to_owned(),
                record_format_version: "5.1".to_owned(),
                record: serde_json::json!({
                    "containers": {
                        "cna": {
                            "affected": [{
                                "vendor": "Example Vendor",
                                "product": "left-pad",
                                "packageName": "left-pad",
                                "collectionURL": "https://registry.npmjs.org",
                                "versions": [{
                                    "version": "1.2.3",
                                    "status": "affected"
                                }]
                            }]
                        }
                    }
                }),
                deleted: false,
                first_seen_at: now,
                last_seen_at: now,
                updated_at: now,
            }]])
            .append_query_results([Vec::<std::collections::BTreeMap<String, Value>>::new()])
            .into_connection();
        let context = ApiCtx::for_test(db, None);
        let test_context = TestContext::new(
            RestApi::new().expect("API description should build").0,
            context,
            &ConfigDropshot::default(),
            None,
            Logger::root(slog::Discard, o!()),
        );

        let error = test_context
            .client_testctx
            .make_request_no_body(
                http::Method::GET,
                &format!("/package-sources/{id}/exposures"),
                StatusCode::SERVICE_UNAVAILABLE,
            )
            .await
            .expect_err("missing vulnerability data should return 503");

        assert_eq!(
            error.error_code.as_deref(),
            Some("VulnerabilityDataUnavailable")
        );

        test_context.teardown().await;
    }

    #[tokio::test]
    async fn get_package_source_exposures_when_kev_data_is_missing_for_present_cve() {
        let id = Uuid::now_v7();
        let now = Utc::now().fixed_offset();
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![source(id, "completed", None)]])
            .append_query_results([vec![package_source_versions::Model {
                id: 1,
                source_id: 1,
                package_version_id: 1,
            }]])
            .append_query_results([vec![package_versions::Model {
                id: 1,
                package_id: 1,
                version: "1.2.3".to_owned(),
                package_url: "pkg:npm/left-pad@1.2.3".to_owned(),
                source_repository: None,
                source_repository_tag: None,
            }]])
            .append_query_results([vec![package_source_edges::Model {
                id: 1,
                source_id: 1,
                parent_package_version_id: None,
                child_package_version_id: 1,
                root_dependency_kind: Some("dependencies".to_owned()),
                declared_dependency: Some("left-pad".to_owned()),
                declared_specification: Some("1.2.3".to_owned()),
            }]])
            .append_query_results([Vec::<package_source_warnings::Model>::new()])
            .append_query_results([vec![cve_list_records::Model {
                cve_id: "CVE-2026-1234".to_owned(),
                record_format_version: "5.1".to_owned(),
                record: serde_json::json!({
                    "containers": {
                        "cna": {
                            "affected": [{
                                "vendor": "Example Vendor",
                                "product": "left-pad",
                                "packageName": "left-pad",
                                "collectionURL": "https://registry.npmjs.org",
                                "versions": [{
                                    "version": "1.2.3",
                                    "status": "affected"
                                }]
                            }]
                        }
                    }
                }),
                deleted: false,
                first_seen_at: now,
                last_seen_at: now,
                updated_at: now,
            }]])
            .append_query_results([Vec::<std::collections::BTreeMap<String, Value>>::new()])
            .into_connection();
        let context = ApiCtx::for_test(db, None);
        let test_context = TestContext::new(
            RestApi::new().expect("API description should build").0,
            context,
            &ConfigDropshot::default(),
            None,
            Logger::root(slog::Discard, o!()),
        );

        let error = test_context
            .client_testctx
            .make_request_no_body(
                http::Method::GET,
                &format!("/package-sources/{id}/exposures"),
                StatusCode::SERVICE_UNAVAILABLE,
            )
            .await
            .expect_err("missing KEV data for a present CVE should return 503");

        assert_eq!(
            error.error_code.as_deref(),
            Some("VulnerabilityDataUnavailable")
        );

        test_context.teardown().await;
    }

    #[test]
    fn assessment_diagnostics_escape_hostile_external_content() {
        let mut run = hipcheck_run();
        run.source_repository_url = Some("https://user:password@example.com/source".to_owned());
        run.stdout = Some("first\r\n<script>alert(1)</script>\u{001b}[31m".to_owned());
        run.stderr = Some("second\u{202e}line".to_owned());
        run.error_message = Some("bad\nrequest\u{0000}".to_owned());

        let diagnostics = assessment_diagnostics(&run);

        assert_eq!(
            diagnostics.source_repository_url.as_deref(),
            Some("https://%3Credacted%3E:%3Credacted%3E@example.com/source")
        );
        assert_eq!(
            diagnostics.stdout.as_deref(),
            Some("first\n<script>alert(1)</script>\\u{001b}[31m")
        );
        assert_eq!(diagnostics.stderr.as_deref(), Some("second\\u{202e}line"));
        assert_eq!(
            diagnostics.error_message.as_deref(),
            Some("bad\\nrequest\\u{0000}")
        );
    }

    fn package_source_summary(
        name: &str,
        lifecycle: PackageSourceLifecycle,
        attention: PackageSourceAttention,
    ) -> PackageSourceSummary {
        PackageSourceSummary {
            id: Uuid::now_v7(),
            display_name: name.to_owned(),
            ecosystem: PackageSourceEcosystem::Npm,
            lifecycle,
            created_at: Utc::now(),
            activity_at: Utc::now(),
            resolution_at: None,
            reachable_package_count: None,
            exposure_count: None,
            exposure_status: PackageSourceExposureSummaryStatus::Processing,
            attention,
        }
    }

    #[test]
    fn package_source_list_filters_and_sorts_summary_rows() {
        let failed = package_source_summary(
            "Zulu",
            PackageSourceLifecycle::Failed,
            PackageSourceAttention::Failed,
        );
        let processing = package_source_summary(
            "alpha",
            PackageSourceLifecycle::Processing,
            PackageSourceAttention::None,
        );
        let warning = package_source_summary(
            "Bravo",
            PackageSourceLifecycle::CompletedWithWarnings,
            PackageSourceAttention::Warnings,
        );
        assert!(package_source_summary_matches_filter(
            &failed,
            PackageSourceListFilter::NeedsAttention
        ));
        assert!(package_source_summary_matches_filter(
            &warning,
            PackageSourceListFilter::NeedsAttention
        ));
        assert!(!package_source_summary_matches_filter(
            &processing,
            PackageSourceListFilter::NeedsAttention
        ));
        assert!(package_source_summary_matches_filter(
            &processing,
            PackageSourceListFilter::Processing
        ));

        let mut summaries = vec![failed, processing, warning];
        sort_package_source_summaries(
            &mut summaries,
            PackageSourceListSort::Identity,
            PackageSourceListDirection::Asc,
        );
        assert_eq!(
            summaries
                .iter()
                .map(|summary| summary.display_name.as_str())
                .collect::<Vec<_>>(),
            vec!["Bravo", "Zulu", "alpha"]
        );
    }

    #[tokio::test]
    async fn order_package_source_list_query_adds_source_id_tiebreak_for_identity_sort() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([Vec::<package_sources::Model>::new()])
            .into_connection();

        order_package_source_list_query(
            package_sources::Entity::find(),
            PackageSourceListSort::Identity,
            PackageSourceListDirection::Asc,
        )
        .all(&db)
        .await
        .expect("identity query should execute against the mock database");

        let transaction_log = db.into_transaction_log();
        let sql = &transaction_log[0].statements()[0].sql;
        let primary = sql
            .find(r#""package_sources"."display_name" ASC"#)
            .expect("identity sort should order by display_name");
        let tie_break = sql
            .find(r#""package_sources"."source_id" ASC"#)
            .expect("identity sort should add source_id as a tie-break");

        assert!(
            tie_break > primary,
            "source_id tie-break should appear after display_name ordering: {sql}"
        );
    }

    #[tokio::test]
    async fn order_package_source_list_query_adds_source_id_tiebreak_for_activity_sort() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([Vec::<package_sources::Model>::new()])
            .into_connection();

        order_package_source_list_query(
            package_sources::Entity::find(),
            PackageSourceListSort::Activity,
            PackageSourceListDirection::Asc,
        )
        .all(&db)
        .await
        .expect("activity query should execute against the mock database");

        let transaction_log = db.into_transaction_log();
        let sql = &transaction_log[0].statements()[0].sql;
        let primary = sql
            .find(
                r#"COALESCE("package_sources"."terminal_at", "package_sources"."created_at") ASC"#,
            )
            .expect("activity sort should order by terminal_at/created_at coalesce");
        let tie_break = sql
            .find(r#""package_sources"."source_id" ASC"#)
            .expect("activity sort should add source_id as a tie-break");

        assert!(
            tie_break > primary,
            "source_id tie-break should appear after activity ordering: {sql}"
        );
    }

    #[tokio::test]
    async fn package_source_list_sql_backed_sets_next_cursor_when_more_rows_exist() {
        let first_id = Uuid::now_v7();
        let second_id = Uuid::now_v7();
        let third_id = Uuid::now_v7();
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![
                listed_source(1, first_id, "alpha", "completed"),
                listed_source(2, second_id, "bravo", "completed"),
                listed_source(3, third_id, "charlie", "completed"),
            ]])
            .append_query_results([Vec::<package_source_warnings::Model>::new()])
            .append_query_results([Vec::<package_source_versions::Model>::new()])
            .append_query_results([Vec::<package_versions::Model>::new()])
            .append_query_results([empty_scalar_rows()])
            .append_query_results([empty_scalar_rows()])
            .into_connection();

        let page = package_source_list_sql_backed(
            &db,
            0,
            2,
            None,
            PackageSourceListFilter::All,
            PackageSourceListSort::Identity,
            PackageSourceListDirection::Asc,
        )
        .await
        .expect("sql-backed listing should succeed");

        assert_eq!(page.items.len(), 2);
        assert_eq!(page.next_cursor, Some("2".to_owned()));
        assert_eq!(page.items[0].id, first_id);
        assert_eq!(page.items[1].id, second_id);
    }

    #[tokio::test]
    async fn package_source_list_sql_backed_omits_next_cursor_when_page_is_exhausted() {
        let first_id = Uuid::now_v7();
        let second_id = Uuid::now_v7();
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![
                listed_source(1, first_id, "alpha", "completed"),
                listed_source(2, second_id, "bravo", "completed"),
            ]])
            .append_query_results([Vec::<package_source_warnings::Model>::new()])
            .append_query_results([Vec::<package_source_versions::Model>::new()])
            .append_query_results([Vec::<package_versions::Model>::new()])
            .append_query_results([empty_scalar_rows()])
            .append_query_results([empty_scalar_rows()])
            .into_connection();

        let page = package_source_list_sql_backed(
            &db,
            0,
            2,
            None,
            PackageSourceListFilter::All,
            PackageSourceListSort::Identity,
            PackageSourceListDirection::Asc,
        )
        .await
        .expect("sql-backed listing should succeed");

        assert_eq!(page.items.len(), 2);
        assert_eq!(page.next_cursor, None);
        assert_eq!(page.items[0].id, first_id);
        assert_eq!(page.items[1].id, second_id);
    }

    #[tokio::test]
    async fn package_source_list_uses_in_memory_fallback_for_needs_attention_or_exposures() {
        let attention_id = Uuid::now_v7();
        let normal_id = Uuid::now_v7();
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![
                listed_source(1, attention_id, "attention", "completed"),
                listed_source(2, normal_id, "normal", "processing"),
            ]])
            .append_query_results([Vec::<package_source_warnings::Model>::new()])
            .append_query_results([Vec::<package_source_versions::Model>::new()])
            .append_query_results([Vec::<package_versions::Model>::new()])
            .append_query_results([empty_scalar_rows()])
            .append_query_results([empty_scalar_rows()])
            .into_connection();

        let page = package_source_list(
            &db,
            PackageSourceListQuery {
                cursor: None,
                limit: Some(10),
                query: None,
                filter: Some(PackageSourceListFilter::NeedsAttention),
                sort: Some(PackageSourceListSort::Identity),
                direction: Some(PackageSourceListDirection::Asc),
            },
        )
        .await
        .expect("needs-attention listing should stay on the in-memory fallback path");

        assert_eq!(page.next_cursor, None);
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].id, attention_id);
        assert_eq!(
            page.items[0].attention,
            PackageSourceAttention::ExposureDataUnavailable
        );
    }

    #[test]
    fn package_source_list_validation_bounds_cursors_and_display_names() {
        assert_eq!(parse_package_source_list_cursor(None).unwrap(), 0);
        assert_eq!(parse_package_source_list_cursor(Some("10")).unwrap(), 10);
        parse_package_source_list_cursor(Some("invalid")).unwrap_err();
        parse_package_source_list_cursor(Some("10001")).unwrap_err();
        assert_eq!(
            validated_package_source_display_name("  Example  ".to_owned()).unwrap(),
            "Example"
        );
        validated_package_source_display_name(" ".to_owned()).unwrap_err();
    }

    fn source(
        id: Uuid,
        resolution_status: &str,
        resolution_error: Option<String>,
    ) -> package_sources::Model {
        package_sources::Model {
            id: 1,
            source_id: id.to_string(),
            display_name: "test package source".to_owned(),
            file_name: "package.json".to_owned(),
            file_contents: "{}".to_owned(),
            inferred_type: "npm-package-json".to_owned(),
            resolution_status: resolution_status.to_owned(),
            resolution_error,
            created_at: Utc::now().fixed_offset(),
            attempt_generation: 1,
            cancellation_requested: matches!(resolution_status, "cancelled" | "deleting"),
            terminal_at: matches!(resolution_status, "completed" | "failed" | "cancelled")
                .then(|| Utc::now().fixed_offset()),
            deletion_reason: None,
            next_attempt_at: Utc::now().fixed_offset(),
            lease_expires_at: None,
            failure_kind: None,
            retryable: false,
            automatic_attempt_count: 0,
        }
    }

    fn listed_source(
        row_id: i32,
        id: Uuid,
        display_name: &str,
        resolution_status: &str,
    ) -> package_sources::Model {
        let mut row = source(id, resolution_status, None);
        row.id = row_id;
        row.display_name = display_name.to_owned();
        row
    }

    fn empty_scalar_rows() -> Vec<std::collections::BTreeMap<String, Value>> {
        Vec::new()
    }

    fn input() -> UpgradeAssessmentInput {
        UpgradeAssessmentInput {
        package_source: UpgradeAssessmentPackageSourceInput {
            ecosystem: PackageSourceEcosystem::Npm,
            file_name: "package.json".to_owned(),
            contents:
                r#"{"name":"example-app","version":"1.0.0","dependencies":{"example-package":"1.2.3"}}"#
                    .to_owned(),
        },
        vulnerable_package: Some(UpgradeAssessmentVulnerablePackageInput {
            name: "example-package".to_owned(),
            ecosystem: PackageSourceEcosystem::Npm,
            version: "1.2.3".to_owned(),
            purl: Some("pkg:npm/example-package@1.2.3".to_owned()),
        }),
        cve_linkage: Some(vec!["CVE-2026-1234".to_owned()]),
        kev_linkage: Some(UpgradeAssessmentKevLinkage {
            cve_ids: vec!["CVE-2026-1234".to_owned()],
            known_exploited: Some(true),
            references: vec![
                "https://www.cisa.gov/known-exploited-vulnerabilities-catalog".to_owned(),
            ],
        }),
            candidate_version: Some("1.2.7".to_owned()),
        }
    }

    fn supply_chain_finding(
        effect: &str,
        summary: &str,
    ) -> nv_server_api::UpgradeAssessmentFinding {
        nv_server_api::UpgradeAssessmentFinding {
            id: format!("finding-{effect}"),
            category: UpgradeAssessmentFindingCategory::SupplyChain,
            effect: hipcheck_finding_effect(effect),
            title: "Test supply-chain finding".to_owned(),
            summary: summary.to_owned(),
            severity: None,
            confidence: None,
            evidence_ids: vec!["evidence-1".to_owned()],
        }
    }

    fn discovered_candidate(
        version: &str,
        distance: UpgradeDistance,
        compatibility: ApiCompatibility,
        verdict: UpgradeAssessmentVerdict,
    ) -> DiscoveredAssessmentCandidate {
        DiscoveredAssessmentCandidate {
            candidate: UpgradeCandidate {
                name: NpmPackageName::parse("example-package".to_owned()).unwrap(),
                version: semver::Version::parse(version).unwrap(),
                purl: format!("pkg:npm/example-package@{version}"),
                published_at: None,
                upgrade_distance: distance,
                api_compatibility: compatibility,
                status: CandidateStatus::Included,
            },
            base_verdict: verdict,
        }
    }

    #[test]
    fn discovered_report_persists_patch_minor_and_major_candidates_in_order() {
        let report = discovered_assessment_report(
            UpgradeAssessmentInput {
                candidate_version: None,
                ..input()
            },
            vec![
                discovered_candidate(
                    "1.2.4",
                    UpgradeDistance::Patch,
                    ApiCompatibility::Compatible,
                    UpgradeAssessmentVerdict::Recommended,
                ),
                discovered_candidate(
                    "1.3.0",
                    UpgradeDistance::Minor,
                    ApiCompatibility::Compatible,
                    UpgradeAssessmentVerdict::Recommended,
                ),
                discovered_candidate(
                    "2.0.0",
                    UpgradeDistance::Major,
                    ApiCompatibility::Incompatible,
                    UpgradeAssessmentVerdict::Recommended,
                ),
            ],
        );

        assert_eq!(
            report
                .candidate_versions
                .iter()
                .map(|candidate| candidate.version.as_str())
                .collect::<Vec<_>>(),
            ["1.2.4", "1.3.0", "2.0.0"]
        );
        assert_eq!(
            report.candidate_versions[0].upgrade_distance,
            Some(UpgradeAssessmentUpgradeDistance::Patch)
        );
        assert_eq!(
            report.candidate_versions[1].upgrade_distance,
            Some(UpgradeAssessmentUpgradeDistance::Minor)
        );
        assert_eq!(
            report.candidate_versions[2].upgrade_distance,
            Some(UpgradeAssessmentUpgradeDistance::Major)
        );
        assert!(!report.candidate_versions[0].is_requested_candidate);
        assert!(matches!(
            report.verdict,
            UpgradeAssessmentVerdict::Recommended
        ));
        assert_eq!(
            report.candidate_versions[0].verdict,
            Some(UpgradeAssessmentVerdict::Recommended)
        );
        assert_eq!(
            report.candidate_versions[2].verdict,
            Some(UpgradeAssessmentVerdict::Caution)
        );
    }

    #[test]
    fn discovery_rejects_mismatched_packument_identity() {
        let requested = NpmPackageName::parse("requested-package".to_owned()).unwrap();
        let returned = NpmPackageName::parse("returned-package".to_owned()).unwrap();

        let error = validate_discovery_packument_identity(&requested, &returned)
            .expect_err("mismatched registry metadata must be rejected");

        assert_eq!(
            error.error_code.as_deref(),
            Some("InvalidUpgradeAssessment")
        );
    }

    #[test]
    fn discovered_major_with_blocking_evidence_remains_avoid() {
        let report = discovered_assessment_report(
            UpgradeAssessmentInput {
                candidate_version: None,
                ..input()
            },
            vec![discovered_candidate(
                "2.0.0",
                UpgradeDistance::Major,
                ApiCompatibility::Incompatible,
                UpgradeAssessmentVerdict::Avoid,
            )],
        );

        assert!(matches!(
            report.candidate_versions[0].verdict,
            Some(UpgradeAssessmentVerdict::Avoid)
        ));
    }

    #[test]
    fn explicit_reports_keep_kev_context_independent_of_candidate_verdict() {
        let cases = [
            ("1.2.7", UpgradeAssessmentVerdict::Recommended),
            ("1.2.7", UpgradeAssessmentVerdict::Avoid),
            // A major candidate is cautioned even when its KEV result is
            // otherwise recommended.
            ("2.0.0", UpgradeAssessmentVerdict::Recommended),
        ];

        for (candidate_version, base_verdict) in cases {
            let report = assessment_report(
                UpgradeAssessmentInput {
                    candidate_version: Some(candidate_version.to_owned()),
                    ..input()
                },
                base_verdict,
                Vec::new(),
                Vec::new(),
                None,
            );

            assert_eq!(
                report
                    .input
                    .kev_linkage
                    .as_ref()
                    .and_then(|kev_linkage| kev_linkage.known_exploited),
                Some(true)
            );
            if candidate_version == "2.0.0" {
                assert_eq!(
                    report.candidate_versions[0].upgrade_distance,
                    Some(UpgradeAssessmentUpgradeDistance::Major)
                );
                assert!(matches!(
                    report.candidate_versions[0].verdict,
                    Some(UpgradeAssessmentVerdict::Caution)
                ));
            }
        }
    }

    #[test]
    fn explicit_no_guarantee_patch_candidate_is_cautioned_when_base_verdict_is_recommended() {
        let report = assessment_report(
            UpgradeAssessmentInput {
                vulnerable_package: Some(UpgradeAssessmentVulnerablePackageInput {
                    version: "0.5.0".to_owned(),
                    ..input()
                        .vulnerable_package
                        .expect("test input includes package")
                }),
                candidate_version: Some("0.5.1".to_owned()),
                ..input()
            },
            UpgradeAssessmentVerdict::Recommended,
            Vec::new(),
            Vec::new(),
            None,
        );

        assert_eq!(
            report.candidate_versions[0].upgrade_distance,
            Some(UpgradeAssessmentUpgradeDistance::Patch)
        );
        assert!(report.candidate_versions[0].is_requested_candidate);
        assert!(matches!(
            report.verdict,
            UpgradeAssessmentVerdict::Recommended
        ));
        assert!(matches!(
            report.candidate_versions[0].verdict,
            Some(UpgradeAssessmentVerdict::Caution)
        ));
    }

    #[test]
    fn explicit_unparsable_versions_require_compatibility_review() {
        let report = assessment_report(
            UpgradeAssessmentInput {
                vulnerable_package: Some(UpgradeAssessmentVulnerablePackageInput {
                    version: "latest".to_owned(),
                    ..input()
                        .vulnerable_package
                        .expect("test input includes package")
                }),
                candidate_version: Some("next".to_owned()),
                ..input()
            },
            UpgradeAssessmentVerdict::Recommended,
            Vec::new(),
            Vec::new(),
            None,
        );

        assert_eq!(
            report.candidate_versions[0].upgrade_distance,
            Some(UpgradeAssessmentUpgradeDistance::Unknown)
        );
        assert_eq!(
            report.candidate_versions[0].verdict,
            Some(UpgradeAssessmentVerdict::Caution)
        );
    }

    #[test]
    fn blocking_supply_chain_finding_moves_candidate_to_avoid() {
        let report = assessment_report(
            input(),
            UpgradeAssessmentVerdict::Recommended,
            vec![supply_chain_finding(
                "blocking",
                "source and package contents differ",
            )],
            Vec::new(),
            Some("INVESTIGATE".to_owned()),
        );

        assert!(matches!(report.verdict, UpgradeAssessmentVerdict::Avoid));
        assert!(matches!(
            report.candidate_versions[0].verdict,
            Some(UpgradeAssessmentVerdict::Avoid)
        ));
    }

    #[test]
    fn review_supply_chain_finding_moves_acceptable_candidate_to_caution() {
        let report = assessment_report(
            input(),
            UpgradeAssessmentVerdict::Recommended,
            vec![supply_chain_finding("review", "release delta needs review")],
            Vec::new(),
            Some("INVESTIGATE".to_owned()),
        );

        assert!(matches!(report.verdict, UpgradeAssessmentVerdict::Caution));
        assert_eq!(report.findings[0].summary, "release delta needs review");
        assert_eq!(
            report.findings[0].effect,
            UpgradeAssessmentFindingEffect::Review
        );
    }

    #[test]
    fn missing_important_supply_chain_check_produces_unknown_verdict() {
        let report = assessment_report(
            input(),
            UpgradeAssessmentVerdict::Recommended,
            vec![supply_chain_finding(
                "missing-check",
                "source/tag comparison could not run",
            )],
            Vec::new(),
            Some("INVESTIGATE".to_owned()),
        );

        assert!(matches!(report.verdict, UpgradeAssessmentVerdict::Unknown));
        assert_eq!(
            report.findings[0].effect,
            UpgradeAssessmentFindingEffect::MissingCheck
        );
    }

    #[test]
    fn failed_hipcheck_run_produces_missing_check_and_unknown_verdict() {
        let stored = nv_common::hipcheck::storage::StoredHipcheckRun {
            run: hipcheck_run(),
            checks: Vec::new(),
            concerns: Vec::new(),
            findings: Vec::new(),
        };

        let report = report_from_hipcheck(
            input(),
            Uuid::now_v7(),
            UpgradeAssessmentVerdict::Recommended,
            &stored,
        );

        assert!(matches!(report.verdict, UpgradeAssessmentVerdict::Unknown));
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].id, "hipcheck-incomplete-run");
        assert_eq!(
            report.findings[0].effect,
            UpgradeAssessmentFindingEffect::MissingCheck
        );
    }

    #[test]
    fn hipcheck_pass_does_not_override_upgrade_domain_blocker() {
        let report = assessment_report(
            input(),
            UpgradeAssessmentVerdict::Avoid,
            Vec::new(),
            Vec::new(),
            Some("PASS".to_owned()),
        );

        assert!(matches!(report.verdict, UpgradeAssessmentVerdict::Avoid));
        assert!(report.caveats.iter().any(|caveat| {
            caveat
                .summary
                .contains("Hipcheck policy recommendation was PASS")
        }));
    }

    #[test]
    fn assessment_report_explains_supply_chain_findings_used_for_verdict() {
        let report = assessment_report(
            input(),
            UpgradeAssessmentVerdict::Recommended,
            vec![supply_chain_finding(
                "review",
                "new install script requires review",
            )],
            vec![UpgradeAssessmentEvidence {
                id: "evidence-1".to_owned(),
                source_type: UpgradeAssessmentEvidenceSourceType::Hipcheck,
                title: "Assessment evidence".to_owned(),
                summary: "Evidence backing the supply-chain finding".to_owned(),
                url: Some("/assessments/example/evidence".to_owned()),
                details: None,
                raw_source_identifiers: Vec::new(),
            }],
            Some("INVESTIGATE".to_owned()),
        );

        assert_eq!(
            report.findings[0].effect,
            UpgradeAssessmentFindingEffect::Review
        );
        assert_eq!(
            report.findings[0].summary,
            "new install script requires review"
        );
        assert_eq!(report.findings[0].evidence_ids, vec!["evidence-1"]);
        assert_eq!(
            report.evidence[0].url.as_deref(),
            Some("/assessments/example/evidence")
        );
    }

    #[test]
    fn input_serializes_package_source_and_vulnerable_package_fields() {
        let serialized = serde_json::to_value(input()).expect("input must serialize");

        assert_eq!(serialized["packageSource"]["fileName"], "package.json");
        assert_eq!(serialized["vulnerablePackage"]["name"], "example-package");
        assert_eq!(serialized["cveLinkage"][0], "CVE-2026-1234");
        assert_eq!(serialized["candidateVersion"], "1.2.7");
    }

    #[test]
    fn initial_report_retains_reviewable_input_and_caveats() {
        let report = initial_assessment_report(input());
        let serialized = serde_json::to_value(&report).expect("report must serialize");

        assert_eq!(
            serialized["input"]["vulnerablePackage"]["name"],
            "example-package"
        );
        assert_eq!(
            serialized["input"]["packageSource"]["fileName"],
            "package.json"
        );
        assert_eq!(serialized["verdict"], "unknown");
        assert_eq!(serialized["candidateVersions"][0]["version"], "1.2.7");
        assert_eq!(
            serialized["candidateVersions"][0]["upgradeDistance"],
            "patch"
        );
        assert_eq!(serialized["candidateVersions"][0]["verdict"], "unknown");
        assert!(
            serialized["caveats"]
                .as_array()
                .is_some_and(|caveats| !caveats.is_empty())
        );
        assert!(serialized.get("findings").is_some());
        assert!(serialized.get("evidence").is_some());
    }

    #[test]
    fn initial_report_marks_major_candidates_as_caution() {
        let mut assessment_input = input();
        assessment_input.candidate_version = Some("2.0.0".to_owned());

        let report = initial_assessment_report(assessment_input);

        assert_eq!(
            report.candidate_versions[0].upgrade_distance,
            Some(UpgradeAssessmentUpgradeDistance::Major)
        );
        assert_eq!(
            report.candidate_versions[0].verdict,
            Some(UpgradeAssessmentVerdict::Caution)
        );
    }

    #[test]
    fn initial_report_marks_no_guarantee_minor_candidates_as_caution() {
        let mut assessment_input = input();
        assessment_input.vulnerable_package = Some(UpgradeAssessmentVulnerablePackageInput {
            version: "0.5.0".to_owned(),
            ..assessment_input
                .vulnerable_package
                .clone()
                .expect("test input includes package")
        });
        assessment_input.candidate_version = Some("0.6.0".to_owned());

        let report = initial_assessment_report(assessment_input);

        assert_eq!(
            report.candidate_versions[0].upgrade_distance,
            Some(UpgradeAssessmentUpgradeDistance::Minor)
        );
        assert_eq!(
            report.candidate_versions[0].verdict,
            Some(UpgradeAssessmentVerdict::Caution)
        );
    }

    #[test]
    fn persisted_completed_assessment_deserializes_to_completed_status() {
        let id = Uuid::now_v7();
        let report = serde_json::to_value(initial_assessment_report(input()))
            .expect("report must serialize");
        let timestamp = Utc
            .with_ymd_and_hms(2026, 9, 1, 12, 0, 0)
            .unwrap()
            .fixed_offset();
        let model = upgrade_assessments::Model {
            id: id.to_string(),
            package_name: "example-package".to_owned(),
            current_version: "1.2.3".to_owned(),
            trigger_kind: "cve".to_owned(),
            trigger_reference: "CVE-2026-1234".to_owned(),
            candidate_version: Some("1.2.7".to_owned()),
            status: "completed".to_owned(),
            created_at: timestamp,
            finished_at: Some(timestamp),
            report: Some(report),
            error: None,
        };

        let status = assessment_status(model).expect("stored report must be readable");
        assert_eq!(status.id, id);
        assert_eq!(status.status, UpgradeAssessmentWorkflowStatus::Completed);
        assert_eq!(status.completed_at, Some(timestamp.with_timezone(&Utc)));
        assert!(status.error.is_none());
    }

    #[test]
    fn persisted_failed_assessment_deserializes_to_failed_status() {
        let id = Uuid::now_v7();
        let timestamp = Utc
            .with_ymd_and_hms(2026, 9, 1, 12, 0, 0)
            .unwrap()
            .fixed_offset();
        let model = upgrade_assessments::Model {
            id: id.to_string(),
            package_name: "example-package".to_owned(),
            current_version: "1.2.3".to_owned(),
            trigger_kind: "cve".to_owned(),
            trigger_reference: "CVE-2026-1234".to_owned(),
            candidate_version: None,
            status: "failed".to_owned(),
            created_at: timestamp,
            finished_at: None,
            report: None,
            error: Some("analysis unavailable".to_owned()),
        };

        let status = assessment_status(model).expect("stored failure must be readable");
        assert_eq!(status.id, id);
        assert_eq!(status.status, UpgradeAssessmentWorkflowStatus::Failed);
        assert_eq!(status.created_at, timestamp.with_timezone(&Utc));
        assert_eq!(status.updated_at, timestamp.with_timezone(&Utc));
        assert_eq!(status.completed_at, None);
        assert_eq!(
            status
                .error
                .as_ref()
                .and_then(|error| error.code.as_deref()),
            None
        );
        assert_eq!(
            status.error.as_ref().map(|error| error.message.as_str()),
            Some("analysis unavailable")
        );
    }

    #[tokio::test]
    async fn get_upgrade_assessment_returns_failed_status_with_error_payload() {
        let id = Uuid::now_v7();
        let timestamp = Utc
            .with_ymd_and_hms(2026, 9, 1, 12, 0, 0)
            .unwrap()
            .fixed_offset();
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![upgrade_assessments::Model {
                id: id.to_string(),
                package_name: "example-package".to_owned(),
                current_version: "1.2.3".to_owned(),
                trigger_kind: "cve".to_owned(),
                trigger_reference: "CVE-2026-1234".to_owned(),
                candidate_version: Some("1.2.7".to_owned()),
                status: "failed".to_owned(),
                created_at: timestamp,
                finished_at: Some(timestamp),
                report: None,
                error: Some("analysis unavailable".to_owned()),
            }]])
            .into_connection();
        let context = ApiCtx::for_test(db, None);
        let test_context = TestContext::new(
            RestApi::new().expect("API description should build").0,
            context,
            &ConfigDropshot::default(),
            None,
            Logger::root(slog::Discard, o!()),
        );

        let mut response = test_context
            .client_testctx
            .make_request_no_body(
                http::Method::GET,
                &format!("/upgrade-assessments/{id}"),
                StatusCode::OK,
            )
            .await
            .expect("failed persisted upgrade assessment should return its status payload");
        let body: serde_json::Value = read_json(&mut response).await;

        let expected_timestamp =
            serde_json::to_value(timestamp.with_timezone(&Utc)).expect("timestamp must serialize");

        assert_eq!(body["id"], id.to_string());
        assert_eq!(body["status"], "failed");
        assert_eq!(body["createdAt"], expected_timestamp);
        assert_eq!(body["updatedAt"], expected_timestamp);
        assert!(body["completedAt"].is_null());
        assert!(body["error"]["code"].is_null());
        assert_eq!(body["error"]["message"], "analysis unavailable");
        test_context.teardown().await;
    }

    #[test]
    fn pending_assessment_result_is_unavailable() {
        let id = Uuid::now_v7();
        let timestamp = Utc
            .with_ymd_and_hms(2026, 9, 1, 12, 0, 0)
            .unwrap()
            .fixed_offset();
        let model = upgrade_assessments::Model {
            id: id.to_string(),
            package_name: "example-package".to_owned(),
            current_version: "1.2.3".to_owned(),
            trigger_kind: "cve".to_owned(),
            trigger_reference: "CVE-2026-1234".to_owned(),
            candidate_version: Some("1.2.7".to_owned()),
            status: "pending".to_owned(),
            created_at: timestamp,
            finished_at: None,
            report: None,
            error: None,
        };

        let error =
            assessment_result(model).expect_err("pending assessments must not expose a result yet");

        assert_eq!(error.status_code, ClientErrorStatusCode::CONFLICT);
        assert_eq!(
            error.error_code.as_deref(),
            Some("UpgradeAssessmentResultUnavailable")
        );
        assert_eq!(
            error.external_message,
            "upgrade assessment result is not available until processing completes"
        );
    }

    #[test]
    fn failed_assessment_result_is_unavailable() {
        let id = Uuid::now_v7();
        let timestamp = Utc
            .with_ymd_and_hms(2026, 9, 1, 12, 0, 0)
            .unwrap()
            .fixed_offset();
        let model = upgrade_assessments::Model {
            id: id.to_string(),
            package_name: "example-package".to_owned(),
            current_version: "1.2.3".to_owned(),
            trigger_kind: "cve".to_owned(),
            trigger_reference: "CVE-2026-1234".to_owned(),
            candidate_version: Some("1.2.7".to_owned()),
            status: "failed".to_owned(),
            created_at: timestamp,
            finished_at: Some(timestamp),
            report: None,
            error: Some("analysis unavailable".to_owned()),
        };

        let error = assessment_result(model)
            .expect_err("failed assessments must not expose a result payload");

        assert_eq!(error.status_code, ClientErrorStatusCode::CONFLICT);
        assert_eq!(
            error.error_code.as_deref(),
            Some("UpgradeAssessmentResultUnavailable")
        );
        assert_eq!(
            error.external_message,
            "upgrade assessment did not complete successfully"
        );
    }

    #[test]
    fn completed_assessment_without_report_returns_internal_error() {
        let id = Uuid::now_v7();
        let timestamp = Utc
            .with_ymd_and_hms(2026, 9, 1, 12, 0, 0)
            .unwrap()
            .fixed_offset();
        let model = upgrade_assessments::Model {
            id: id.to_string(),
            package_name: "example-package".to_owned(),
            current_version: "1.2.3".to_owned(),
            trigger_kind: "cve".to_owned(),
            trigger_reference: "CVE-2026-1234".to_owned(),
            candidate_version: Some("1.2.7".to_owned()),
            status: "completed".to_owned(),
            created_at: timestamp,
            finished_at: Some(timestamp),
            report: None,
            error: None,
        };

        let error = assessment_result(model)
            .expect_err("completed assessments without a stored report must fail");

        assert_eq!(error.status_code, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            error.internal_message,
            "completed upgrade assessment has no report"
        );
    }

    #[test]
    fn assessment_result_rejects_invalid_persisted_status() {
        let id = Uuid::now_v7();
        let timestamp = Utc
            .with_ymd_and_hms(2026, 9, 1, 12, 0, 0)
            .unwrap()
            .fixed_offset();
        let model = upgrade_assessments::Model {
            id: id.to_string(),
            package_name: "example-package".to_owned(),
            current_version: "1.2.3".to_owned(),
            trigger_kind: "cve".to_owned(),
            trigger_reference: "CVE-2026-1234".to_owned(),
            candidate_version: Some("1.2.7".to_owned()),
            status: "mystery".to_owned(),
            created_at: timestamp,
            finished_at: None,
            report: None,
            error: None,
        };

        let error =
            assessment_result(model).expect_err("unknown persisted statuses must be rejected");

        assert_eq!(error.status_code, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            error.internal_message,
            "upgrade assessment has an invalid status"
        );
    }

    #[tokio::test]
    async fn get_upgrade_assessment_result_returns_not_found_for_unknown_id() {
        let id = Uuid::now_v7();
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([Vec::<upgrade_assessments::Model>::new()])
            .into_connection();
        let context = ApiCtx::for_test(db, None);
        let test_context = TestContext::new(
            RestApi::new().expect("API description should build").0,
            context,
            &ConfigDropshot::default(),
            None,
            Logger::root(slog::Discard, o!()),
        );

        let error = test_context
            .client_testctx
            .make_request_no_body(
                http::Method::GET,
                &format!("/upgrade-assessments/{id}/result"),
                StatusCode::NOT_FOUND,
            )
            .await
            .expect_err("unknown upgrade assessment should return 404");

        assert_eq!(error.error_code, None);
        assert_eq!(error.message, "Not Found");
        test_context.teardown().await;
    }

    #[tokio::test]
    async fn get_upgrade_assessment_result_returns_conflict_until_completed() {
        let id = Uuid::now_v7();
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![upgrade_assessments::Model {
                id: id.to_string(),
                package_name: "example-package".to_owned(),
                current_version: "1.2.3".to_owned(),
                trigger_kind: "cve".to_owned(),
                trigger_reference: "CVE-2026-1234".to_owned(),
                candidate_version: Some("1.2.7".to_owned()),
                status: "processing".to_owned(),
                created_at: Utc::now().fixed_offset(),
                finished_at: None,
                report: None,
                error: None,
            }]])
            .into_connection();
        let context = ApiCtx::for_test(db, None);
        let test_context = TestContext::new(
            RestApi::new().expect("API description should build").0,
            context,
            &ConfigDropshot::default(),
            None,
            Logger::root(slog::Discard, o!()),
        );

        let error = test_context
            .client_testctx
            .make_request_no_body(
                http::Method::GET,
                &format!("/upgrade-assessments/{id}/result"),
                StatusCode::CONFLICT,
            )
            .await
            .expect_err("incomplete upgrade assessment should return 409");

        assert_eq!(
            error.error_code.as_deref(),
            Some("UpgradeAssessmentResultUnavailable")
        );
        assert!(
            error
                .message
                .contains("upgrade assessment result is not available until processing completes")
        );
        test_context.teardown().await;
    }

    #[tokio::test]
    async fn get_upgrade_assessment_result_returns_conflict_for_failed_assessments() {
        let id = Uuid::now_v7();
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![upgrade_assessments::Model {
                id: id.to_string(),
                package_name: "example-package".to_owned(),
                current_version: "1.2.3".to_owned(),
                trigger_kind: "cve".to_owned(),
                trigger_reference: "CVE-2026-1234".to_owned(),
                candidate_version: Some("1.2.7".to_owned()),
                status: "failed".to_owned(),
                created_at: Utc::now().fixed_offset(),
                finished_at: Some(Utc::now().fixed_offset()),
                report: None,
                error: Some("analysis unavailable".to_owned()),
            }]])
            .into_connection();
        let context = ApiCtx::for_test(db, None);
        let test_context = TestContext::new(
            RestApi::new().expect("API description should build").0,
            context,
            &ConfigDropshot::default(),
            None,
            Logger::root(slog::Discard, o!()),
        );

        let error = test_context
            .client_testctx
            .make_request_no_body(
                http::Method::GET,
                &format!("/upgrade-assessments/{id}/result"),
                StatusCode::CONFLICT,
            )
            .await
            .expect_err("failed upgrade assessment result should remain unavailable");

        assert_eq!(
            error.error_code.as_deref(),
            Some("UpgradeAssessmentResultUnavailable")
        );
        assert!(
            error
                .message
                .contains("upgrade assessment did not complete successfully")
        );
        test_context.teardown().await;
    }

    #[tokio::test]
    async fn get_upgrade_assessment_result_returns_completed_payload_when_available() {
        let id = Uuid::now_v7();
        let report = initial_assessment_report(input());
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![upgrade_assessments::Model {
                id: id.to_string(),
                package_name: "example-package".to_owned(),
                current_version: "1.2.3".to_owned(),
                trigger_kind: "cve".to_owned(),
                trigger_reference: "CVE-2026-1234".to_owned(),
                candidate_version: Some("1.2.7".to_owned()),
                status: "completed".to_owned(),
                created_at: Utc::now().fixed_offset(),
                finished_at: Some(Utc::now().fixed_offset()),
                report: Some(serde_json::to_value(&report).expect("report must serialize")),
                error: None,
            }]])
            .into_connection();
        let context = ApiCtx::for_test(db, None);
        let test_context = TestContext::new(
            RestApi::new().expect("API description should build").0,
            context,
            &ConfigDropshot::default(),
            None,
            Logger::root(slog::Discard, o!()),
        );

        let mut response = test_context
            .client_testctx
            .make_request_no_body(
                http::Method::GET,
                &format!("/upgrade-assessments/{id}/result"),
                StatusCode::OK,
            )
            .await
            .expect("completed upgrade assessment should return 200");
        let body: serde_json::Value = read_json(&mut response).await;

        assert_eq!(
            body["input"]["vulnerablePackage"]["name"],
            "example-package"
        );
        assert_eq!(body["input"]["candidateVersion"], "1.2.7");
        assert_eq!(body["verdict"], "unknown");
        assert_eq!(body["candidateVersions"][0]["version"], "1.2.7");
        test_context.teardown().await;
    }

    #[test]
    fn candidate_version_without_vulnerable_package_is_rejected() {
        let mut request = input();
        request.vulnerable_package = None;
        request.cve_linkage = None;
        request.kev_linkage = None;

        let error = validate_upgrade_assessment_request(&request)
            .expect_err("candidateVersion without vulnerablePackage must be rejected");

        assert_eq!(error.status_code, ClientErrorStatusCode::BAD_REQUEST);
        assert_eq!(
            error.error_code.as_deref(),
            Some("InvalidUpgradeAssessment")
        );
        assert_eq!(
            error.external_message,
            "vulnerablePackage must be provided when cveLinkage, kevLinkage, or candidateVersion is supplied"
        );
    }

    #[test]
    fn cve_linkage_without_vulnerable_package_is_rejected() {
        let mut request = input();
        request.vulnerable_package = None;
        request.candidate_version = None;
        request.kev_linkage = None;

        let error = validate_upgrade_assessment_request(&request)
            .expect_err("cveLinkage without vulnerablePackage must be rejected");

        assert_eq!(error.status_code, ClientErrorStatusCode::BAD_REQUEST);
        assert_eq!(
            error.error_code.as_deref(),
            Some("InvalidUpgradeAssessment")
        );
        assert_eq!(
            error.external_message,
            "vulnerablePackage must be provided when cveLinkage, kevLinkage, or candidateVersion is supplied"
        );
    }

    #[test]
    fn kev_linkage_without_vulnerable_package_is_rejected() {
        let mut request = input();
        request.vulnerable_package = None;
        request.candidate_version = None;
        request.cve_linkage = None;

        let error = validate_upgrade_assessment_request(&request)
            .expect_err("kevLinkage without vulnerablePackage must be rejected");

        assert_eq!(error.status_code, ClientErrorStatusCode::BAD_REQUEST);
        assert_eq!(
            error.error_code.as_deref(),
            Some("InvalidUpgradeAssessment")
        );
        assert_eq!(
            error.external_message,
            "vulnerablePackage must be provided when cveLinkage, kevLinkage, or candidateVersion is supplied"
        );
    }

    #[test]
    fn empty_candidate_version_is_rejected() {
        let mut request = input();
        request.candidate_version = Some("   ".to_owned());

        let error = validate_upgrade_assessment_request(&request)
            .expect_err("empty candidateVersion must be rejected");

        assert_eq!(error.status_code, ClientErrorStatusCode::BAD_REQUEST);
        assert_eq!(
            error.error_code.as_deref(),
            Some("InvalidUpgradeAssessment")
        );
        assert_eq!(
            error.external_message,
            "candidateVersion must not be empty when supplied"
        );
    }

    #[test]
    fn invalid_semver_candidate_version_is_rejected() {
        let mut request = input();
        request.candidate_version = Some("not-a-semver".to_owned());

        let error = validate_upgrade_assessment_request(&request)
            .expect_err("invalid candidateVersion semver must be rejected");

        assert_eq!(error.status_code, ClientErrorStatusCode::BAD_REQUEST);
        assert_eq!(
            error.error_code.as_deref(),
            Some("InvalidUpgradeAssessment")
        );
        assert_eq!(
            error.external_message,
            "candidateVersion must be a valid semantic version when supplied"
        );
    }

    #[test]
    fn mismatched_vulnerable_package_purl_is_rejected() {
        let mut request = input();
        request.vulnerable_package = Some(UpgradeAssessmentVulnerablePackageInput {
            purl: Some("pkg:npm/other-package@9.9.9".to_owned()),
            ..request
                .vulnerable_package
                .clone()
                .expect("test input includes vulnerable package")
        });

        let error = validate_upgrade_assessment_request(&request)
            .expect_err("mismatched vulnerablePackage.purl must be rejected");

        assert_eq!(error.status_code, ClientErrorStatusCode::BAD_REQUEST);
        assert_eq!(
            error.error_code.as_deref(),
            Some("InvalidUpgradeAssessment")
        );
        assert_eq!(
            error.external_message,
            "vulnerablePackage.purl must match vulnerablePackage.name and vulnerablePackage.version"
        );
    }

    #[test]
    fn lifecycle_persists_request_then_completed_report() {
        let id = Uuid::now_v7();
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([
                vec![upgrade_assessments::Model {
                    id: id.to_string(),
                    package_name: "example-package".to_owned(),
                    current_version: "1.2.3".to_owned(),
                    trigger_kind: "cve".to_owned(),
                    trigger_reference: "CVE-2026-1234".to_owned(),
                    candidate_version: Some("1.2.7".to_owned()),
                    status: "processing".to_owned(),
                    created_at: Utc::now().fixed_offset(),
                    finished_at: None,
                    report: None,
                    error: None,
                }],
                vec![upgrade_assessments::Model {
                    id: id.to_string(),
                    package_name: "example-package".to_owned(),
                    current_version: "1.2.3".to_owned(),
                    trigger_kind: "cve".to_owned(),
                    trigger_reference: "CVE-2026-1234".to_owned(),
                    candidate_version: Some("1.2.7".to_owned()),
                    status: "completed".to_owned(),
                    created_at: Utc::now().fixed_offset(),
                    finished_at: Some(Utc::now().fixed_offset()),
                    report: Some(serde_json::to_value(initial_assessment_report(input())).unwrap()),
                    error: None,
                }],
            ])
            .append_exec_results([
                MockExecResult {
                    last_insert_id: 0,
                    rows_affected: 1,
                },
                MockExecResult {
                    last_insert_id: 0,
                    rows_affected: 1,
                },
            ])
            .into_connection();
        let runtime = tokio::runtime::Runtime::new().expect("test runtime must start");

        runtime.block_on(async {
            create_upgrade_assessment(&db, id, &input())
                .await
                .expect("request must persist");
            complete_upgrade_assessment(&db, id, input(), UpgradeAssessmentVerdict::Unknown)
                .await
                .expect("report must persist");
        });

        let transaction_log = db.into_transaction_log();
        assert_eq!(transaction_log.len(), 2);
        assert!(
            transaction_log[0].statements()[0]
                .sql
                .contains(r#"INSERT INTO "public"."upgrade_assessments""#)
        );
        assert!(
            transaction_log[1].statements()[0]
                .sql
                .contains(r#"UPDATE "public"."upgrade_assessments""#)
        );
        assert!(
            transaction_log[1].statements()[0]
                .sql
                .contains(r#""status" = $"#)
        );
        assert!(
            transaction_log[1].statements()[0]
                .sql
                .contains(r#""report" = $"#)
        );
    }

    #[test]
    fn lifecycle_marks_queue_setup_failure_as_failed() {
        let id = Uuid::now_v7();
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([
                vec![upgrade_assessments::Model {
                    id: id.to_string(),
                    package_name: "example-package".to_owned(),
                    current_version: "1.2.3".to_owned(),
                    trigger_kind: "cve".to_owned(),
                    trigger_reference: "CVE-2026-1234".to_owned(),
                    candidate_version: Some("1.2.7".to_owned()),
                    status: "processing".to_owned(),
                    created_at: Utc::now().fixed_offset(),
                    finished_at: None,
                    report: None,
                    error: None,
                }],
                vec![upgrade_assessments::Model {
                    id: id.to_string(),
                    package_name: "example-package".to_owned(),
                    current_version: "1.2.3".to_owned(),
                    trigger_kind: "cve".to_owned(),
                    trigger_reference: "CVE-2026-1234".to_owned(),
                    candidate_version: Some("1.2.7".to_owned()),
                    status: "failed".to_owned(),
                    created_at: Utc::now().fixed_offset(),
                    finished_at: Some(Utc::now().fixed_offset()),
                    report: None,
                    error: Some("assessment queue setup failed".to_owned()),
                }],
            ])
            .append_exec_results([
                MockExecResult {
                    last_insert_id: 0,
                    rows_affected: 1,
                },
                MockExecResult {
                    last_insert_id: 0,
                    rows_affected: 1,
                },
            ])
            .into_connection();
        let runtime = tokio::runtime::Runtime::new().expect("test runtime must start");

        runtime.block_on(async {
            create_upgrade_assessment(&db, id, &input())
                .await
                .expect("request must persist");
            mark_upgrade_assessment_failed(&db, id, "assessment queue setup failed".to_owned())
                .await
                .expect("queue failure must persist a terminal state");
        });

        let transaction_log = db.into_transaction_log();
        assert_eq!(transaction_log.len(), 2);
        assert!(
            transaction_log[1].statements()[0]
                .sql
                .contains(r#"UPDATE "public"."upgrade_assessments""#)
        );
        assert!(
            transaction_log[1].statements()[0]
                .sql
                .contains(r#""status" = $"#)
        );
        assert!(
            transaction_log[1].statements()[0]
                .sql
                .contains(r#""error" = $"#)
        );
    }

    #[tokio::test]
    async fn lookup_returns_a_safe_fixed_diagnostic_never_the_raw_error() {
        let id = Uuid::now_v7();
        let mut row = source(id, "failed", Some("x".repeat(2048)));
        row.failure_kind = Some("dependency-unavailable".to_owned());
        row.retryable = true;
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![row]])
            .append_query_results([Vec::<package_source_versions::Model>::new()])
            .append_query_results([Vec::<package_versions::Model>::new()])
            .append_query_results([Vec::<package_source_edges::Model>::new()])
            .into_connection();

        let status = lookup_package_source(&db, id).await.unwrap().unwrap();

        let PackageSourceStatus::Failed(failed) = status else {
            panic!("expected failed package source status");
        };
        assert_eq!(
            failed.diagnostic,
            FailureKind::DependencyUnavailable.diagnostic()
        );
        assert!(!failed.diagnostic.contains('x'), "raw error text leaked");
        assert_eq!(failed.kind, PackageSourceFailureKind::DependencyUnavailable);
        assert!(failed.retryable);
        assert!(failed.previous_versioned_packages.is_empty());
    }

    #[tokio::test]
    async fn lookup_fails_closed_on_an_unrecognized_failure_kind() {
        let id = Uuid::now_v7();
        let mut row = source(id, "failed", None);
        row.failure_kind = Some("not-a-real-kind".to_owned());
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![row]])
            .into_connection();

        lookup_package_source(&db, id)
            .await
            .expect_err("an unrecognized stored failure kind must not be published");
    }

    #[tokio::test]
    async fn lookup_returns_cancelled_state_with_attempt_and_timestamp() {
        let id = Uuid::now_v7();
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![source(id, "cancelled", None)]])
            .into_connection();

        let status = lookup_package_source(&db, id).await.unwrap().unwrap();

        let PackageSourceStatus::Cancelled(cancelled) = status else {
            panic!("expected cancelled package source status");
        };
        assert_eq!(cancelled.attempt, 1);
        assert!(cancelled.cancelled_at >= cancelled.created_at);
    }

    #[tokio::test]
    async fn lookup_keeps_the_prior_snapshot_visible_after_a_failure() {
        let id = Uuid::now_v7();
        let mut row = source(id, "failed", Some("registry request failed".to_owned()));
        row.failure_kind = Some("dependency-unavailable".to_owned());
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![row]])
            .append_query_results([vec![package_source_versions::Model {
                id: 1,
                source_id: 1,
                package_version_id: 1,
            }]])
            .append_query_results([vec![package_versions::Model {
                id: 1,
                package_id: 1,
                version: "1.0.0".to_owned(),
                package_url: "pkg:npm/root@1.0.0".to_owned(),
                source_repository: None,
                source_repository_tag: None,
            }]])
            .append_query_results([vec![package_source_edges::Model {
                id: 1,
                source_id: 1,
                parent_package_version_id: None,
                child_package_version_id: 1,
                root_dependency_kind: Some("dependencies".to_owned()),
                declared_dependency: Some("root".to_owned()),
                declared_specification: Some("1.0.0".to_owned()),
            }]])
            .into_connection();

        let status = lookup_package_source(&db, id).await.unwrap().unwrap();

        let PackageSourceStatus::Failed(failed) = status else {
            panic!("expected failed package source status");
        };
        assert_eq!(
            failed.diagnostic,
            FailureKind::DependencyUnavailable.diagnostic()
        );
        assert_eq!(
            failed.previous_versioned_packages[0].derivations,
            vec![vec!["<root>".to_owned(), "pkg:npm/root@1.0.0".to_owned()]]
        );
    }

    #[tokio::test]
    async fn lookup_returns_completed_with_persisted_warnings() {
        let id = Uuid::now_v7();
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![source(id, "completed", None)]])
            .append_query_results([vec![package_source_versions::Model {
                id: 1,
                source_id: 1,
                package_version_id: 1,
            }]])
            .append_query_results([vec![package_versions::Model {
                id: 1,
                package_id: 1,
                version: "1.0.0".to_owned(),
                package_url: "pkg:npm/%40scope/root@1.0.0".to_owned(),
                source_repository: None,
                source_repository_tag: None,
            }]])
            .append_query_results([vec![package_source_edges::Model {
                id: 1,
                source_id: 1,
                parent_package_version_id: None,
                child_package_version_id: 1,
                root_dependency_kind: Some("dependencies".to_owned()),
                declared_dependency: Some("root".to_owned()),
                declared_specification: Some("1.0.0".to_owned()),
            }]])
            .append_query_results([vec![package_source_warnings::Model {
                id: 1,
                source_id: 1,
                declared_by_purl: None,
                dependency_name: "local-package".to_owned(),
                specification_kind: "file".to_owned(),
                message: "File dependencies cannot be resolved.".to_owned(),
            }]])
            .into_connection();

        let status = lookup_package_source(&db, id).await.unwrap().unwrap();

        let PackageSourceStatus::CompletedWithWarnings(completed) = status else {
            panic!("expected completed-with-warnings package source status");
        };
        assert!(!completed.warnings_truncated);
        assert_eq!(completed.warnings.len(), 1);
        assert_eq!(completed.warnings[0].dependency_name, "local-package");
        assert_eq!(
            completed.warnings[0].message,
            "File dependencies cannot be resolved."
        );
        assert_eq!(completed.versioned_packages[0].name, "@scope/root");
        assert_eq!(
            completed.versioned_packages[0].derivations,
            vec![vec![
                "<root>".to_owned(),
                "pkg:npm/%40scope/root@1.0.0".to_owned(),
            ]]
        );
    }
}
