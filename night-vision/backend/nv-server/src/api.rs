//! Defines the API endpoints for the Night Vision server.

pub mod ctx;

mod cve_worker;

use crate::{api::ctx::ApiCtx, error::FatalError};
use camino::{Utf8Path, Utf8PathBuf};
// We'd prefer to use `jiff` over `chrono`, but `dropshot` depends on
// an old version of `schemars` that doesn't support `jiff`. When we
// can use a newer version of `schemars`, we should switch to using
// `jiff`.
use chrono::Utc;
use dropshot::{
    ClientErrorStatusCode, HttpError, HttpResponseAccepted, HttpResponseOk, Path, RequestContext,
    ServerBuilder, UntypedBody,
};
use nv_common::{
    config::Config,
    cve::storage::{
        has_cve_list_records, last_successful_cve_list_sync_commit, latest_cve_list_sync_run,
    },
    db::entities::cve_list_sync_runs::Model as CveListSyncRun,
    npm::elaboration::{
        NpmRegistryClient, elaborate,
        storage::{
            bounded_diagnostic, persist_completed_elaboration, persisted_package_versions,
            record_elaboration_failure,
        },
    },
    npm::package_json::NpmPackageJson,
};
use nv_server_api::{
    CveIngestHealth, CveListSyncRunHealth, Health, NvServerApi, PackageSource,
    PackageSourceEcosystem, PackageSourcePathParams, PackageSourceStatus,
    PackageSourceStatusCompleted, PackageSourceStatusCompletedWithWarnings,
    PackageSourceStatusFailed, PackageSourceStatusProcessing, PackageSourceWarning,
    PostPackageSourceBody, PostPackageSourceResponse, VersionedPackage,
    nv_server_api_mod::api_description,
};
use percent_encoding::percent_decode_str;
use sea_orm::{
    ActiveModelTrait as _, ActiveValue::Set, ColumnTrait as _, DatabaseConnection,
    EntityTrait as _, QueryFilter as _, QueryOrder as _, QuerySelect as _,
};
use slog::Logger;
use std::{fs::File, time::Duration};
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
        ctx: RequestContext<Self::Context>,
    ) -> Result<HttpResponseOk<Health>, HttpError> {
        let cve_ingest = cve_ingest_health(ctx.context().db()).await?;

        Ok(HttpResponseOk(Health {
            status: "ok".to_owned(),
            cve_ingest,
        }))
    }

    async fn post_package_source(
        ctx: RequestContext<Self::Context>,
        body_param: UntypedBody,
    ) -> Result<HttpResponseAccepted<PostPackageSourceResponse>, HttpError> {
        validate_package_source_media_type(
            ctx.request
                .headers()
                .get("content-type")
                .and_then(|value| value.to_str().ok()),
        )?;
        let context = ctx.context();
        let db = context.db();
        let body = serde_json::from_slice::<PostPackageSourceBody>(body_param.as_bytes()).map_err(
            |_| {
                HttpError::for_bad_request(
                    Some("InvalidPackageSourceRequest".to_owned()),
                    "package-source request body must be valid JSON".to_owned(),
                )
            },
        )?;
        let file_name = body.file_name;
        let contents = body.contents;
        NpmPackageJson::parse_package_json(contents.as_bytes())
            .map_err(|error| HttpError::for_bad_request(None, error.to_string()))?;

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
                    "error" => error,
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
}

fn validate_package_source_media_type(content_type: Option<&str>) -> Result<(), HttpError> {
    let Some(content_type) = content_type else {
        return Err(unsupported_package_source_media_type());
    };
    let media_type = content_type
        .split_once(';')
        .map_or(content_type, |(media_type, _)| media_type)
        .trim();

    if media_type.eq_ignore_ascii_case("application/json") {
        Ok(())
    } else {
        Err(unsupported_package_source_media_type())
    }
}

fn unsupported_package_source_media_type() -> HttpError {
    HttpError::for_client_error(
        Some("UnsupportedPackageSourceMediaType".to_owned()),
        ClientErrorStatusCode::UNSUPPORTED_MEDIA_TYPE,
        "package-source requests must use application/json".to_owned(),
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
    .map_err(|error| HttpError::for_internal_error(error.to_string()))?;
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
        .map_err(|error| error.to_string())
        .and_then(|source| {
            NpmRegistryClient::new(registry_url, max_packument_bytes, limits.request_timeout)
                .map(|client| (source, client))
                .map_err(|error| error.to_string())
        });
    let result = match result {
        Ok((source, client)) => elaborate(&source, std::sync::Arc::new(client), limits)
            .await
            .map_err(|error| error.to_string()),
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
            Err(error) => {
                let diagnostic = format!("failed to persist completed elaboration: {error}");
                record_terminal_failure_with_retry(db, source_id, &diagnostic)
                    .await
                    .map_err(|failure| format!("{diagnostic}; {failure}"))
            }
        },
        Err(error) => record_terminal_failure_with_retry(db, source_id, &error).await,
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
            Err(error) => last_error = Some(format!("{error:?}")),
        }
        if attempt < TERMINAL_PERSISTENCE_ATTEMPTS {
            tokio::time::sleep(TERMINAL_PERSISTENCE_RETRY_DELAY).await;
        }
    }
    let error = last_error.expect("at least one terminal persistence attempt was made");
    Err(format!(
        "failed to record elaboration failure after {TERMINAL_PERSISTENCE_ATTEMPTS} attempts: {error}"
    ))
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
        .map_err(|error| HttpError::for_internal_error(error.to_string()))?
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
        .map_err(|error| HttpError::for_internal_error(error.to_string()))?
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
        .map_err(|error| HttpError::for_internal_error(error.to_string()))?;
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
        .map_err(|error| HttpError::for_internal_error(error.to_string()))?;
    let latest_successful_commit = last_successful_cve_list_sync_commit(db)
        .await
        .map_err(|error| HttpError::for_internal_error(error.to_string()))?
        .map(|commit| commit.as_str().to_owned());
    let latest_run = latest_cve_list_sync_run(db)
        .await
        .map_err(|error| HttpError::for_internal_error(error.to_string()))?
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

#[cfg(test)]
mod tests {
    use super::*;
    use nv_common::{
        db::entities::{
            package_source_edges, package_source_warnings, package_sources, package_versions,
        },
        npm::elaboration::storage::MAX_FAILURE_DIAGNOSTIC_BYTES,
    };
    use sea_orm::{DbBackend, DbErr, MockDatabase, MockExecResult};

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

    #[tokio::test]
    async fn lookup_returns_bounded_failed_diagnostic() {
        let id = Uuid::now_v7();
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([vec![source(id, "failed", Some("x".repeat(2048)))]])
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
            .append_query_results([vec![package_versions::Model {
                id: 1,
                package_id: 1,
                source_id: 1,
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
            .append_query_results([vec![package_versions::Model {
                id: 1,
                package_id: 1,
                source_id: 1,
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
    async fn background_failures_record_failed_lifecycle_state() {
        for error in [
            "failed to retrieve packument for root: registry request failed",
            "elaboration exceeded the package limit",
        ] {
            let db = MockDatabase::new(DbBackend::Postgres)
                .append_exec_results([MockExecResult {
                    last_insert_id: 0,
                    rows_affected: 1,
                }])
                .into_connection();

            finalize_package_source_elaboration(&db, 1, Err(error.to_owned()))
                .await
                .expect("failed elaboration records its terminal state");

            let statements = db
                .into_transaction_log()
                .into_iter()
                .flat_map(|entry| entry.statements().to_vec())
                .map(|statement| statement.sql)
                .collect::<Vec<_>>();
            assert_eq!(statements.len(), 1);
            assert!(statements[0].contains("package_sources"));
            assert!(!statements[0].contains("DELETE"));
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

        let error = finalize_package_source_elaboration(&db, 1, Err("registry failed".to_owned()))
            .await
            .expect_err("unrecorded terminal failure is returned to the task");

        assert!(error.contains("failed to record elaboration failure"));
        assert!(error.contains("database unavailable"));
    }
}
