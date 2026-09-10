//! Defines the API endpoints for the Night Vision server.

pub mod ctx;

mod cve_worker;

use crate::{
    api::ctx::ApiCtx,
    error::{ExternalOperation, FatalError},
};
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
    cve::kev::{KevNpmMatchStatus, ReachableNpmPackageVersion, kev_affected_npm_package_version},
    cve::storage::{
        has_cve_list_records, last_successful_cve_list_sync_commit, latest_cve_list_sync_run,
    },
    db::entities::{
        cve_list_sync_runs::Model as CveListSyncRun, hipcheck_runs, upgrade_assessments,
    },
    hipcheck::{
        assessment::{execute_queued_assessment, queue_assessment, queue_assessment_with_id},
        storage::load_hipcheck_run_by_assessment_id,
    },
    npm::{
        candidates::{
            ApiCompatibility, CandidateStatus, UpgradeDistance, api_compatibility,
            upgrade_distance, validate_explicit_candidate,
        },
        elaboration::{
            NpmRegistryClient, PackageVersion, PackumentProvider as _, elaborate,
            normalize_repository_url,
            storage::{
                bounded_diagnostic, persist_assessment_target, persist_completed_elaboration,
                persisted_package_versions, record_elaboration_failure,
            },
        },
        package_json::NpmPackageJson,
        purl::NpmPackagePurl,
        types::NpmPackageName,
    },
};
use nv_server_api::{
    AssessmentCheck, AssessmentDiagnostics, AssessmentEvidence, AssessmentEvidenceQuery,
    AssessmentFinding, AssessmentPathParams, AssessmentStatus, CveIngestHealth,
    CveListSyncRunHealth, Health, HealthDiagnostics, NvServerApi, PackageSource,
    PackageSourceEcosystem, PackageSourcePathParams, PackageSourceStatus,
    PackageSourceStatusCompleted, PackageSourceStatusCompletedWithWarnings,
    PackageSourceStatusFailed, PackageSourceStatusProcessing, PackageSourceWarning,
    PostAssessmentBody, PostAssessmentResponse, PostPackageSourceBody, PostPackageSourceResponse,
    PostUpgradeAssessmentBody, PostUpgradeAssessmentResponse, UpgradeAssessmentApiCompatibility,
    UpgradeAssessmentCandidate, UpgradeAssessmentCompleted, UpgradeAssessmentConfidence,
    UpgradeAssessmentDependencyDelta, UpgradeAssessmentFailed, UpgradeAssessmentInput,
    UpgradeAssessmentPathParams, UpgradeAssessmentProcessing, UpgradeAssessmentReport,
    UpgradeAssessmentStatus, UpgradeAssessmentTrigger, UpgradeAssessmentUpgradeDistance,
    UpgradeAssessmentVerdict, UpgradeAssessmentVulnerabilityContext, VersionedPackage,
    nv_server_api_mod::api_description,
};
use percent_encoding::percent_decode_str;
use sea_orm::{
    ActiveModelTrait as _, ActiveValue::Set, ColumnTrait as _, DatabaseConnection,
    EntityTrait as _, QueryFilter as _, QueryOrder as _, QuerySelect as _,
};
use secrecy::{ExposeSecret as _, SecretString};
use slog::Logger;
use std::{fs::File, time::Duration};
use subtle::ConstantTimeEq as _;
use uuid::Uuid;

const TERMINAL_PERSISTENCE_ATTEMPTS: usize = 3;
const TERMINAL_PERSISTENCE_RETRY_DELAY: Duration = Duration::from_millis(100);

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
        let cve_list_worker_config = config.cve_list_worker_config();
        let _cve_list_worker = cve_worker::spawn_cve_list_worker(
            ctx.db().clone(),
            cve_list_worker_config,
            log.clone(),
        );
        let _kev_worker =
            nv_common::kev::spawn_kev_sync_worker(config, ctx.db().clone(), log.clone());

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
        let cve_ingest = cve_ingest_health(ctx.context().db()).await?;

        Ok(HttpResponseOk(HealthDiagnostics {
            status: "ok".to_owned(),
            cve_ingest,
        }))
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
        NpmPackageJson::parse_package_json(contents.as_bytes())
            .map_err(|_| invalid_package_source_contents())?;

        let admission = context.try_admit_package_elaboration().ok_or_else(|| {
            HttpError::for_unavail(
                Some("PackageElaborationCapacity".to_owned()),
                "package-source elaboration capacity is exhausted".to_owned(),
            )
        })?;
        let stored = store_validated_package_source(db, file_name, contents).await?;
        let worker_db = db.clone();
        let registry_url = context.npm_registry_url().clone();
        let limits = context.package_elaboration_limits();
        let max_packument_bytes = context.package_elaboration_max_packument_bytes();
        let log = ctx
            .log
            .new(slog::o!("package_source_id" => stored.id.to_string()));
        tokio::spawn(async move {
            let _admission = admission;
            if let Err(error) = run_package_source_elaboration(
                worker_db,
                stored.database_id,
                stored.contents,
                registry_url,
                limits,
                max_packument_bytes,
            )
            .await
            {
                slog::error!(
                    log,
                    "package-source elaboration could not record its terminal state";
                    "failure_kind" => error,
                );
            }
        });
        let uuid = stored.id;
        let resp = HttpResponseAccepted(PostPackageSourceResponse { id: uuid });

        Ok(resp)
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
            if execute_queued_assessment(&db, &queued, &runner)
                .await
                .is_err()
            {
                slog::error!(
                    log,
                    "assessment could not persist terminal state";
                    "failure_kind" => "terminal-state-persistence",
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
            affected_purl: stored.run.affected_purl,
            target: stored.run.target_purl,
            source_repository_url: stored.run.source_repository_url,
            recommendation: stored.run.policy_recommendation,
            finding_count: stored.findings.len(),
            exit_status: stored.run.exit_status,
            error_kind: stored.run.error_kind,
            error_message: stored.run.error_message,
            retryable: stored.run.retryable,
        }))
    }

    async fn get_assessment_evidence(
        ctx: RequestContext<Self::Context>,
        path_params: Path<AssessmentPathParams>,
        query: Query<AssessmentEvidenceQuery>,
    ) -> Result<HttpResponseOk<AssessmentEvidence>, HttpError> {
        let id = path_params.into_inner().id;
        let stored = load_hipcheck_run_by_assessment_id(ctx.context().db(), &id)
            .await
            .map_err(|_| internal_server_error())?
            .ok_or_else(|| HttpError::for_not_found(None, format!("unknown assessment {id}")))?;
        let include_raw = query.into_inner().include_raw_hipcheck.unwrap_or(false);
        Ok(HttpResponseOk(AssessmentEvidence {
            id,
            affected_purl: stored.run.affected_purl.clone(),
            diagnostics: assessment_diagnostics(&stored.run),
            checks: stored
                .checks
                .into_iter()
                .map(|check| AssessmentCheck {
                    state: check.state,
                    effect: check.effect,
                    summary: check.summary,
                })
                .collect(),
            findings: stored
                .findings
                .into_iter()
                .map(|finding| AssessmentFinding {
                    kind: finding.kind,
                    effect: finding.effect,
                    severity: finding.severity,
                    summary: finding.summary,
                })
                .collect(),
            raw_hipcheck: include_raw.then_some(stored.run.raw_json).flatten(),
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
        let admission = input
            .candidate_version
            .as_ref()
            .map(|_| {
                context.try_admit_hipcheck().ok_or_else(|| {
                    HttpError::for_unavail(
                        Some("AssessmentCapacity".to_owned()),
                        "assessment capacity is exhausted".to_owned(),
                    )
                })
            })
            .transpose()?;
        let candidate = match input.candidate_version.as_deref() {
            Some(candidate_version) => {
                let package_name = NpmPackageName::parse(input.package_name.clone())
                    .map_err(invalid_upgrade_request)?;
                let affected_purl =
                    PackageVersion::from_npm(&package_name, &input.current_version).purl();
                let target_purl = PackageVersion::from_npm(&package_name, candidate_version).purl();
                let target_purl =
                    validate_and_persist_upgrade_target(context, &affected_purl, &target_purl)
                        .await?;
                Some((affected_purl, target_purl))
            }
            None => None,
        };
        create_upgrade_assessment(context.db(), id, &input).await?;
        let queued = match candidate.as_ref() {
            Some((affected_purl, target_purl)) => {
                match queue_assessment_with_id(context.db(), id, affected_purl, target_purl).await {
                    Ok(queued) => Some(queued),
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
            None => None,
        };

        // The row is inserted before spawning work.  Therefore a client can always
        // poll the durable processing state, even when it races this task.
        let db = context.db().clone();
        let runner = context.hipcheck_runner_config();
        tokio::spawn(async move {
            let result = match queued {
                Some(queued) => {
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
                None => {
                    complete_upgrade_assessment(&db, id, input, UpgradeAssessmentVerdict::Unknown)
                        .await
                }
            };
            if let Err(error) = result {
                // A failure to record the failure is only possible when the database itself
                // is unavailable; the original processing row remains available for recovery.
                let _ = mark_upgrade_assessment_failed(&db, id, error.to_string()).await;
            }
        });

        Ok(HttpResponseAccepted(PostUpgradeAssessmentResponse { id }))
    }

    async fn get_upgrade_assessment(
        ctx: RequestContext<Self::Context>,
        path_params: Path<UpgradeAssessmentPathParams>,
    ) -> Result<HttpResponseOk<UpgradeAssessmentStatus>, HttpError> {
        let id = path_params.into_inner().id;
        let assessment = upgrade_assessments::Entity::find_by_id(id.to_string())
            .one(ctx.context().db())
            .await
            .map_err(internal_error)?
            .ok_or_else(|| {
                HttpError::for_not_found(None, format!("unknown upgrade assessment {id}"))
            })?;
        Ok(HttpResponseOk(assessment_status(assessment)?))
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
    let repository = packument
        .versions
        .get(&candidate.version)
        .and_then(|version| version.repository.as_ref())
        .or(packument.repository.as_ref())
        .and_then(|repository| normalize_repository_url(&repository.url));
    let target_release = PackageVersion::from_npm(&target.name, &target.version);
    persist_assessment_target(context.db(), &target_release, repository)
        .await
        .map_err(|error| HttpError::for_internal_error(error.to_string()))?;
    Ok(target_release.purl())
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

fn assessment_diagnostics(run: &hipcheck_runs::Model) -> AssessmentDiagnostics {
    AssessmentDiagnostics {
        source_repository_url: run.source_repository_url.clone(),
        stdout: run.stdout.clone(),
        stdout_truncated: run.stdout_truncated,
        stderr: run.stderr.clone(),
        stderr_truncated: run.stderr_truncated,
        exit_status: run.exit_status,
        error_kind: run.error_kind.clone(),
        error_message: run.error_message.clone(),
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
        nv_common::hipcheck::assessment::AssessmentError::Database(_) => internal_server_error(),
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

async fn store_validated_package_source(
    db: &DatabaseConnection,
    file_name: String,
    contents: String,
) -> Result<StoredPackageSource, HttpError> {
    let id = Uuid::now_v7();
    let source = nv_common::db::entities::package_sources::ActiveModel {
        source_id: Set(id.to_string()),
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
    Ok(StoredPackageSource {
        id,
        database_id: source.id,
        contents: source.file_contents,
    })
}

struct StoredPackageSource {
    id: Uuid,
    database_id: i32,
    contents: String,
}

async fn run_package_source_elaboration(
    db: DatabaseConnection,
    source_id: i32,
    contents: String,
    registry_url: url::Url,
    limits: nv_common::npm::elaboration::ElaborationLimits,
    max_packument_bytes: usize,
) -> Result<(), String> {
    let result = NpmPackageJson::parse_package_json(contents.as_bytes())
        .map_err(|_| {
            ExternalOperation::PackageSourceElaboration
                .diagnostic()
                .to_owned()
        })
        .and_then(|source| {
            NpmRegistryClient::new(registry_url, max_packument_bytes, limits.request_timeout)
                .map(|client| (source, client))
                .map_err(|_| {
                    ExternalOperation::PackageSourceElaboration
                        .diagnostic()
                        .to_owned()
                })
        });
    let result = match result {
        Ok((source, client)) => elaborate(&source, std::sync::Arc::new(client), limits)
            .await
            .map_err(|_| {
                ExternalOperation::PackageSourceElaboration
                    .diagnostic()
                    .to_owned()
            }),
        Err(error) => Err(error),
    };
    finalize_package_source_elaboration(&db, source_id, result).await
}

async fn finalize_package_source_elaboration(
    db: &DatabaseConnection,
    source_id: i32,
    result: Result<nv_common::npm::elaboration::ElaborationResult, String>,
) -> Result<(), String> {
    match result {
        Ok(result) => match persist_completed_elaboration(db, source_id, &result).await {
            Ok(()) => Ok(()),
            Err(_) => {
                let diagnostic = ExternalOperation::PackageSourcePersistence.diagnostic();
                record_terminal_failure_with_retry(db, source_id, diagnostic)
                    .await
                    .map_err(|_| diagnostic.to_owned())
            }
        },
        Err(_) => {
            record_terminal_failure_with_retry(
                db,
                source_id,
                ExternalOperation::PackageSourceElaboration.diagnostic(),
            )
            .await
        }
    }
}

async fn record_terminal_failure_with_retry(
    db: &DatabaseConnection,
    source_id: i32,
    diagnostic: &str,
) -> Result<(), String> {
    let mut last_error = None;
    for attempt in 1..=TERMINAL_PERSISTENCE_ATTEMPTS {
        match record_elaboration_failure(db, source_id, diagnostic).await {
            Ok(()) => return Ok(()),
            Err(_) => last_error = Some(()),
        }
        if attempt < TERMINAL_PERSISTENCE_ATTEMPTS {
            tokio::time::sleep(TERMINAL_PERSISTENCE_RETRY_DELAY).await;
        }
    }
    last_error.expect("at least one terminal persistence attempt was made");
    Err(ExternalOperation::PackageSourcePersistence
        .diagnostic()
        .to_owned())
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
    let failure_diagnostic = match source.resolution_status.as_str() {
        "pending" => {
            return Ok(Some(PackageSourceStatus::Processing(
                PackageSourceStatusProcessing { id, created_at },
            )));
        }
        "failed" => Some(bounded_diagnostic(
            source
                .resolution_error
                .as_deref()
                .unwrap_or("Elaboration failed."),
        )),
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

    if let Some(diagnostic) = failure_diagnostic {
        return Ok(Some(PackageSourceStatus::Failed(
            PackageSourceStatusFailed {
                id,
                created_at,
                diagnostic,
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
                source: package_source(source.file_name, source.file_contents),
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
            source: package_source(source.file_name, source.file_contents),
            versioned_packages,
            warnings,
            warnings_truncated,
        },
    )))
}

fn package_source(file_name: String, contents: String) -> PackageSource {
    PackageSource {
        ecosystem: PackageSourceEcosystem::Npm,
        file_name,
        contents,
    }
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

async fn cve_ingest_health(db: &DatabaseConnection) -> Result<CveIngestHealth, HttpError> {
    let records_available = has_cve_list_records(db)
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
        latest_successful_commit,
        latest_run,
    })
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

fn validate_upgrade_assessment_request(body: &PostUpgradeAssessmentBody) -> Result<(), HttpError> {
    if NpmPackageName::parse(body.package_name.clone()).is_err() {
        return Err(HttpError::for_bad_request(
            None,
            "packageName must be a valid NPM package name".to_owned(),
        ));
    }
    if semver::Version::parse(&body.current_version).is_err() {
        return Err(HttpError::for_bad_request(
            None,
            "currentVersion must be a valid semantic version".to_owned(),
        ));
    }
    if body
        .candidate_version
        .as_deref()
        .is_some_and(|version| version.trim().is_empty())
    {
        return Err(HttpError::for_bad_request(
            None,
            "candidateVersion must not be empty when supplied".to_owned(),
        ));
    }
    if body
        .candidate_version
        .as_deref()
        .is_some_and(|version| semver::Version::parse(version).is_err())
    {
        return Err(HttpError::for_bad_request(
            None,
            "candidateVersion must be a valid semantic version when supplied".to_owned(),
        ));
    }
    match &body.trigger {
        UpgradeAssessmentTrigger::Cve { cve_id } if !is_cve_id(cve_id) => Err(
            HttpError::for_bad_request(None, "trigger.cve_id must be a CVE identifier".to_owned()),
        ),
        UpgradeAssessmentTrigger::Exposure { exposure_id } if exposure_id.trim().is_empty() => Err(
            HttpError::for_bad_request(None, "trigger.exposure_id must not be empty".to_owned()),
        ),
        _ => Ok(()),
    }
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
    UpgradeAssessmentInput {
        ecosystem: PackageSourceEcosystem::Npm,
        package_name: body.package_name.clone(),
        current_version: body.current_version.clone(),
        trigger: body.trigger.clone(),
        candidate_version: body.candidate_version.clone(),
    }
}

async fn create_upgrade_assessment(
    db: &DatabaseConnection,
    id: Uuid,
    input: &UpgradeAssessmentInput,
) -> Result<(), HttpError> {
    let (trigger_kind, trigger_reference) = trigger_parts(&input.trigger);
    upgrade_assessments::ActiveModel {
        id: Set(id.to_string()),
        package_name: Set(input.package_name.clone()),
        current_version: Set(input.current_version.clone()),
        trigger_kind: Set(trigger_kind.to_owned()),
        trigger_reference: Set(trigger_reference.to_owned()),
        candidate_version: Set(input.candidate_version.clone()),
        status: Set("processing".to_owned()),
        ..Default::default()
    }
    .insert(db)
    .await
    .map_err(internal_error)?;
    Ok(())
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

fn initial_assessment_report(input: UpgradeAssessmentInput) -> UpgradeAssessmentReport {
    assessment_report(input, UpgradeAssessmentVerdict::Unknown, Vec::new(), None)
}

fn report_from_hipcheck(
    input: UpgradeAssessmentInput,
    assessment_id: Uuid,
    base_verdict: UpgradeAssessmentVerdict,
    stored: &nv_common::hipcheck::storage::StoredHipcheckRun,
) -> UpgradeAssessmentReport {
    let evidence_link = format!("/assessments/{assessment_id}/evidence");
    let mut findings: Vec<_> = stored
        .findings
        .iter()
        .map(|finding| nv_server_api::UpgradeAssessmentFinding {
            effect: finding.effect.clone(),
            summary: finding.summary.clone(),
            evidence_links: vec![evidence_link.clone()],
        })
        .collect();
    if stored.run.status != "completed" {
        findings.push(nv_server_api::UpgradeAssessmentFinding {
            effect: "missing-check".to_owned(),
            summary: stored.run.error_message.clone().unwrap_or_else(|| {
                "Hipcheck did not produce complete supply-chain evidence.".to_owned()
            }),
            evidence_links: vec![evidence_link],
        });
    }
    assessment_report(
        input,
        base_verdict,
        findings,
        stored.run.policy_recommendation.clone(),
    )
}

fn assessment_report(
    input: UpgradeAssessmentInput,
    base_verdict: UpgradeAssessmentVerdict,
    supply_chain_findings: Vec<nv_server_api::UpgradeAssessmentFinding>,
    hipcheck_recommendation: Option<String>,
) -> UpgradeAssessmentReport {
    let verdict = upgrade_assessment_verdict(&base_verdict, &supply_chain_findings);
    let candidate_caveats: Vec<_> = supply_chain_findings
        .iter()
        .map(|finding| finding.summary.clone())
        .collect();
    let candidate_versions = input
        .candidate_version
        .iter()
        .map(|version| {
            let mut candidate = assessment_candidate(&input.current_version, version);
            let is_major = matches!(
                candidate.upgrade_distance,
                UpgradeAssessmentUpgradeDistance::Major
            );
            candidate.verdict = if is_major
                && (matches!(verdict, UpgradeAssessmentVerdict::Recommended)
                    || (matches!(verdict, UpgradeAssessmentVerdict::Unknown)
                        && supply_chain_findings.is_empty()))
            {
                UpgradeAssessmentVerdict::Caution
            } else {
                verdict.clone()
            };
            if is_major {
                candidate.caveats.extend(candidate_caveats.clone());
            } else {
                candidate.caveats = candidate_caveats.clone();
            }
            candidate
        })
        .collect();
    let mut caveats = Vec::new();
    if supply_chain_findings.is_empty() {
        caveats.push("Candidate analysis has not yet produced supply-chain findings.".to_owned());
    }
    if let Some(recommendation) = hipcheck_recommendation {
        caveats.push(format!(
            "Hipcheck policy recommendation was {recommendation}; Night Vision used the normalized findings above when determining this verdict."
        ));
    }
    UpgradeAssessmentReport {
        vulnerability_context: UpgradeAssessmentVulnerabilityContext {
            trigger: input.trigger.clone(),
            kev_linked: matches!(base_verdict, UpgradeAssessmentVerdict::Recommended)
                .then_some(true),
        },
        input,
        verdict: verdict.clone(),
        candidate_versions,
        dependency_delta: UpgradeAssessmentDependencyDelta {
            added: Vec::new(),
            removed: Vec::new(),
            changed: Vec::new(),
        },
        evidence_links: supply_chain_findings
            .iter()
            .flat_map(|finding| finding.evidence_links.iter().cloned())
            .collect(),
        supply_chain_findings,
        confidence: assessment_confidence(&verdict),
        caveats,
    }
}

/// Apply supply-chain evidence without allowing it to weaken a stricter
/// vulnerability or upgrade-domain verdict.
fn upgrade_assessment_verdict(
    base_verdict: &UpgradeAssessmentVerdict,
    findings: &[nv_server_api::UpgradeAssessmentFinding],
) -> UpgradeAssessmentVerdict {
    if matches!(base_verdict, UpgradeAssessmentVerdict::Avoid)
        || findings.iter().any(|finding| finding.effect == "blocking")
    {
        return UpgradeAssessmentVerdict::Avoid;
    }
    if matches!(base_verdict, UpgradeAssessmentVerdict::Unknown)
        || findings
            .iter()
            .any(|finding| finding.effect == "missing-check")
    {
        return UpgradeAssessmentVerdict::Unknown;
    }
    if matches!(base_verdict, UpgradeAssessmentVerdict::Caution)
        || findings.iter().any(|finding| finding.effect == "review")
    {
        return UpgradeAssessmentVerdict::Caution;
    }
    UpgradeAssessmentVerdict::Recommended
}

fn assessment_confidence(verdict: &UpgradeAssessmentVerdict) -> UpgradeAssessmentConfidence {
    match verdict {
        UpgradeAssessmentVerdict::Recommended => UpgradeAssessmentConfidence::Medium,
        UpgradeAssessmentVerdict::Caution | UpgradeAssessmentVerdict::Avoid => {
            UpgradeAssessmentConfidence::Medium
        }
        UpgradeAssessmentVerdict::Unknown => UpgradeAssessmentConfidence::Unknown,
    }
}

fn assessment_candidate(
    current_version: &str,
    candidate_version: &str,
) -> UpgradeAssessmentCandidate {
    let (upgrade_distance, api_compatibility) = match (
        semver::Version::parse(current_version),
        semver::Version::parse(candidate_version),
    ) {
        (Ok(current), Ok(candidate)) => (
            assessment_upgrade_distance(upgrade_distance(&current, &candidate)),
            assessment_api_compatibility(api_compatibility(&current, &candidate)),
        ),
        _ => (
            UpgradeAssessmentUpgradeDistance::Unknown,
            UpgradeAssessmentApiCompatibility::Unknown,
        ),
    };
    let is_major = matches!(upgrade_distance, UpgradeAssessmentUpgradeDistance::Major);
    UpgradeAssessmentCandidate {
        version: candidate_version.to_owned(),
        upgrade_distance,
        api_compatibility,
        verdict: if is_major {
            UpgradeAssessmentVerdict::Caution
        } else {
            UpgradeAssessmentVerdict::Unknown
        },
        caveats: if is_major {
            vec![
                "Major upgrades require application compatibility review and cannot be recommended by Night Vision."
                    .to_owned(),
            ]
        } else {
            vec!["Candidate analysis has not yet been populated from NPM metadata.".to_owned()]
        },
    }
}

fn assessment_upgrade_distance(value: UpgradeDistance) -> UpgradeAssessmentUpgradeDistance {
    match value {
        UpgradeDistance::Patch => UpgradeAssessmentUpgradeDistance::Patch,
        UpgradeDistance::Minor => UpgradeAssessmentUpgradeDistance::Minor,
        UpgradeDistance::Major => UpgradeAssessmentUpgradeDistance::Major,
    }
}

fn assessment_api_compatibility(value: ApiCompatibility) -> UpgradeAssessmentApiCompatibility {
    match value {
        ApiCompatibility::Compatible => UpgradeAssessmentApiCompatibility::Compatible,
        ApiCompatibility::Incompatible => UpgradeAssessmentApiCompatibility::Incompatible,
        ApiCompatibility::NoGuarantee => UpgradeAssessmentApiCompatibility::NoGuarantee,
    }
}

fn assessment_status(
    assessment: upgrade_assessments::Model,
) -> Result<UpgradeAssessmentStatus, HttpError> {
    let id = Uuid::parse_str(&assessment.id).map_err(internal_error)?;
    let created_at = utc(assessment.created_at);
    let input = input_from_model(&assessment)?;
    match assessment.status.as_str() {
        "processing" => Ok(UpgradeAssessmentStatus::Processing(
            UpgradeAssessmentProcessing {
                id,
                created_at,
                input,
            },
        )),
        "completed" => {
            let finished_at = assessment.finished_at.ok_or_else(|| {
                internal_error("completed upgrade assessment has no completion time")
            })?;
            let report = assessment
                .report
                .ok_or_else(|| internal_error("completed upgrade assessment has no report"))?;
            let report = serde_json::from_value(report).map_err(internal_error)?;
            Ok(UpgradeAssessmentStatus::Completed(
                UpgradeAssessmentCompleted {
                    id,
                    created_at,
                    completed_at: utc(finished_at),
                    report,
                },
            ))
        }
        "failed" => {
            let finished_at = assessment
                .finished_at
                .ok_or_else(|| internal_error("failed upgrade assessment has no failure time"))?;
            let error = assessment
                .error
                .ok_or_else(|| internal_error("failed upgrade assessment has no error"))?;
            Ok(UpgradeAssessmentStatus::Failed(UpgradeAssessmentFailed {
                id,
                created_at,
                failed_at: utc(finished_at),
                input,
                error,
            }))
        }
        _ => Err(internal_error("upgrade assessment has an invalid status")),
    }
}

fn input_from_model(
    assessment: &upgrade_assessments::Model,
) -> Result<UpgradeAssessmentInput, HttpError> {
    let trigger = match assessment.trigger_kind.as_str() {
        "cve" => UpgradeAssessmentTrigger::Cve {
            cve_id: assessment.trigger_reference.clone(),
        },
        "exposure" => UpgradeAssessmentTrigger::Exposure {
            exposure_id: assessment.trigger_reference.clone(),
        },
        _ => {
            return Err(internal_error(
                "upgrade assessment has an invalid trigger kind",
            ));
        }
    };
    Ok(UpgradeAssessmentInput {
        ecosystem: PackageSourceEcosystem::Npm,
        package_name: assessment.package_name.clone(),
        current_version: assessment.current_version.clone(),
        trigger,
        candidate_version: assessment.candidate_version.clone(),
    })
}

fn trigger_parts(trigger: &UpgradeAssessmentTrigger) -> (&str, &str) {
    match trigger {
        UpgradeAssessmentTrigger::Cve { cve_id } => ("cve", cve_id),
        UpgradeAssessmentTrigger::Exposure { exposure_id } => ("exposure", exposure_id),
    }
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
    use nv_common::{
        db::entities::{
            package_source_edges, package_source_versions, package_source_warnings,
            package_sources, package_versions,
        },
        npm::elaboration::storage::MAX_FAILURE_DIAGNOSTIC_BYTES,
    };
    use sea_orm::{DbBackend, DbErr, MockDatabase, MockExecResult, Value};
    use slog::{Logger, o};

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
    async fn health_routes_serve_public_and_operator_responses() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([
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
                    "latestSuccessfulCommit": null,
                    "latestRun": null,
                },
            })
        );

        test_context.teardown().await;
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
                    "packageName": "example-package",
                    "currentVersion": "1.2.3",
                    "candidateVersion": "1.2.4",
                    "trigger": {
                        "kind": "cve",
                        "cve_id": "CVE-2026-1234"
                    }
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

    fn source(
        id: Uuid,
        resolution_status: &str,
        resolution_error: Option<String>,
    ) -> package_sources::Model {
        package_sources::Model {
            id: 1,
            source_id: id.to_string(),
            file_name: "package.json".to_owned(),
            file_contents: "{}".to_owned(),
            inferred_type: "npm-package-json".to_owned(),
            resolution_status: resolution_status.to_owned(),
            resolution_error,
            created_at: Utc::now().fixed_offset(),
        }
    }

    fn input() -> UpgradeAssessmentInput {
        UpgradeAssessmentInput {
            ecosystem: PackageSourceEcosystem::Npm,
            package_name: "example-package".to_owned(),
            current_version: "1.2.3".to_owned(),
            trigger: UpgradeAssessmentTrigger::Cve {
                cve_id: "CVE-2026-1234".to_owned(),
            },
            candidate_version: Some("1.2.7".to_owned()),
        }
    }

    fn supply_chain_finding(
        effect: &str,
        summary: &str,
    ) -> nv_server_api::UpgradeAssessmentFinding {
        nv_server_api::UpgradeAssessmentFinding {
            effect: effect.to_owned(),
            summary: summary.to_owned(),
            evidence_links: vec!["/assessments/example/evidence".to_owned()],
        }
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
            Some("INVESTIGATE".to_owned()),
        );

        assert!(matches!(report.verdict, UpgradeAssessmentVerdict::Avoid));
        assert!(matches!(
            report.candidate_versions[0].verdict,
            UpgradeAssessmentVerdict::Avoid
        ));
    }

    #[test]
    fn review_supply_chain_finding_moves_acceptable_candidate_to_caution() {
        let report = assessment_report(
            input(),
            UpgradeAssessmentVerdict::Recommended,
            vec![supply_chain_finding("review", "release delta needs review")],
            Some("INVESTIGATE".to_owned()),
        );

        assert!(matches!(report.verdict, UpgradeAssessmentVerdict::Caution));
        assert_eq!(
            report.candidate_versions[0].caveats,
            vec!["release delta needs review"]
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
            Some("INVESTIGATE".to_owned()),
        );

        assert!(matches!(report.verdict, UpgradeAssessmentVerdict::Unknown));
        assert!(matches!(
            report.confidence,
            UpgradeAssessmentConfidence::Unknown
        ));
    }

    #[test]
    fn hipcheck_pass_does_not_override_upgrade_domain_blocker() {
        let report = assessment_report(
            input(),
            UpgradeAssessmentVerdict::Avoid,
            Vec::new(),
            Some("PASS".to_owned()),
        );

        assert!(matches!(report.verdict, UpgradeAssessmentVerdict::Avoid));
        assert!(
            report
                .caveats
                .iter()
                .any(|caveat| caveat.contains("Hipcheck policy recommendation was PASS"))
        );
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
            Some("INVESTIGATE".to_owned()),
        );

        assert_eq!(report.supply_chain_findings[0].effect, "review");
        assert_eq!(
            report.supply_chain_findings[0].summary,
            "new install script requires review"
        );
        assert_eq!(
            report.supply_chain_findings[0].evidence_links,
            vec!["/assessments/example/evidence"]
        );
    }

    #[test]
    fn upgrade_assessment_trigger_uses_snake_case_fields() {
        let value = serde_json::json!({
            "kind": "cve",
            "cve_id": "CVE-2026-1234",
        });
        let trigger = serde_json::from_value::<UpgradeAssessmentTrigger>(value.clone())
            .expect("snake_case trigger fields must deserialize");

        assert_eq!(serde_json::to_value(trigger).unwrap(), value);
        serde_json::from_value::<UpgradeAssessmentTrigger>(serde_json::json!({
            "kind": "cve",
            "cveId": "CVE-2026-1234",
        }))
        .unwrap_err();
    }

    #[test]
    fn initial_report_retains_reviewable_input_and_caveats() {
        let report = initial_assessment_report(input());
        let serialized = serde_json::to_value(&report).expect("report must serialize");

        assert_eq!(serialized["input"]["packageName"], "example-package");
        assert_eq!(serialized["verdict"], "unknown");
        assert_eq!(serialized["candidateVersions"][0]["version"], "1.2.7");
        assert_eq!(
            serialized["candidateVersions"][0]["upgradeDistance"],
            "patch"
        );
        assert_eq!(
            serialized["candidateVersions"][0]["apiCompatibility"],
            "compatible"
        );
        assert!(
            serialized["caveats"]
                .as_array()
                .is_some_and(|caveats| !caveats.is_empty())
        );
        assert!(serialized.get("dependencyDelta").is_some());
        assert!(serialized.get("supplyChainFindings").is_some());
        assert!(serialized.get("evidenceLinks").is_some());
    }

    #[test]
    fn initial_report_marks_major_candidates_as_caution() {
        let mut assessment_input = input();
        assessment_input.candidate_version = Some("2.0.0".to_owned());

        let report = initial_assessment_report(assessment_input);

        assert_eq!(
            report.candidate_versions[0].upgrade_distance,
            UpgradeAssessmentUpgradeDistance::Major
        );
        assert_eq!(
            report.candidate_versions[0].api_compatibility,
            UpgradeAssessmentApiCompatibility::Incompatible
        );
        assert!(matches!(
            report.candidate_versions[0].verdict,
            UpgradeAssessmentVerdict::Caution
        ));
        assert!(
            report.candidate_versions[0]
                .caveats
                .iter()
                .any(|caveat| caveat.contains("compatibility review"))
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
        assert!(matches!(status, UpgradeAssessmentStatus::Completed(_)));
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
            finished_at: Some(timestamp),
            report: None,
            error: Some("analysis unavailable".to_owned()),
        };

        let status = assessment_status(model).expect("stored failure must be readable");
        assert!(matches!(status, UpgradeAssessmentStatus::Failed(_)));
    }

    #[test]
    fn invalid_requests_are_rejected_before_persistence() {
        let invalid = PostUpgradeAssessmentBody {
            package_name: "example-package".to_owned(),
            current_version: "1.2.3".to_owned(),
            trigger: UpgradeAssessmentTrigger::Cve {
                cve_id: "not-a-cve".to_owned(),
            },
            candidate_version: None,
        };

        assert!(validate_upgrade_assessment_request(&invalid).is_err());
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
    async fn lookup_returns_bounded_failed_diagnostic() {
        let id = Uuid::now_v7();
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![source(id, "failed", Some("x".repeat(2048)))]])
            .append_query_results([Vec::<package_source_versions::Model>::new()])
            .append_query_results([Vec::<package_versions::Model>::new()])
            .append_query_results([Vec::<package_source_edges::Model>::new()])
            .into_connection();

        let status = lookup_package_source(&db, id).await.unwrap().unwrap();

        let PackageSourceStatus::Failed(failed) = status else {
            panic!("expected failed package source status");
        };
        assert!(failed.diagnostic.len() <= MAX_FAILURE_DIAGNOSTIC_BYTES);
        assert!(failed.diagnostic.ends_with("..."));
        assert!(failed.previous_versioned_packages.is_empty());
    }

    #[tokio::test]
    async fn lookup_keeps_the_prior_snapshot_visible_after_a_failure() {
        let id = Uuid::now_v7();
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![source(
                id,
                "failed",
                Some("registry request failed".to_owned()),
            )]])
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
        assert_eq!(failed.diagnostic, "registry request failed");
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

    #[tokio::test]
    async fn background_completion_publishes_a_snapshot() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_exec_results(std::iter::repeat_n(
                MockExecResult {
                    last_insert_id: 0,
                    rows_affected: 1,
                },
                4,
            ))
            .into_connection();

        finalize_package_source_elaboration(
            &db,
            1,
            Ok(nv_common::npm::elaboration::ElaborationResult {
                packages: Vec::new(),
                edges: Vec::new(),
                warnings: Vec::new(),
            }),
        )
        .await
        .expect("successful elaboration publishes its terminal state");

        let statements = db
            .into_transaction_log()
            .into_iter()
            .flat_map(|entry| entry.statements().to_vec())
            .map(|statement| statement.sql)
            .collect::<Vec<_>>();
        assert!(statements.iter().any(|sql| sql.contains("package_sources")));
        assert_eq!(statements.first(), Some(&"BEGIN".to_owned()));
        assert_eq!(statements.last(), Some(&"COMMIT".to_owned()));
    }

    #[tokio::test]
    async fn background_failures_record_a_stable_redacted_diagnostic() {
        for unsafe_error in [
            "failed to retrieve packument for root: registry request failed",
            "password=correct-horse-battery-staple",
        ] {
            let db = MockDatabase::new(DbBackend::Postgres)
                .append_exec_results([MockExecResult {
                    last_insert_id: 0,
                    rows_affected: 1,
                }])
                .into_connection();

            finalize_package_source_elaboration(&db, 1, Err(unsafe_error.to_owned()))
                .await
                .expect("failed elaboration records its terminal state");

            let statements = db
                .into_transaction_log()
                .into_iter()
                .flat_map(|entry| entry.statements().to_vec())
                .collect::<Vec<_>>();
            assert_eq!(statements.len(), 1);
            assert!(statements[0].sql.contains("package_sources"));
            assert!(!statements[0].sql.contains("DELETE"));
            let values = statements[0]
                .values
                .as_ref()
                .expect("failure update must have bound values");
            assert!(values.iter().any(|value| matches!(
                value,
                Value::String(Some(message))
                    if message == ExternalOperation::PackageSourceElaboration.diagnostic()
            )));
            assert!(!values.iter().any(|value| matches!(
                value,
                Value::String(Some(message)) if message == unsafe_error
            )));
        }
    }

    #[tokio::test]
    async fn background_failure_reports_exhausted_terminal_persistence_retries() {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_exec_errors(std::iter::repeat_n(
                DbErr::Custom("database unavailable".to_owned()),
                TERMINAL_PERSISTENCE_ATTEMPTS,
            ))
            .into_connection();

        let error = finalize_package_source_elaboration(
            &db,
            1,
            Err("password=correct-horse-battery-staple".to_owned()),
        )
        .await
        .expect_err("unrecorded terminal failure is returned to the task");

        assert_eq!(
            error,
            ExternalOperation::PackageSourcePersistence.diagnostic()
        );
        assert!(!error.contains("correct-horse-battery-staple"));
    }
}
