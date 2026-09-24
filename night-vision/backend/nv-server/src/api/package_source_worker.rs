//! Durable dispatch of persisted `pending` package sources and recovery of
//! attempts whose lease expired (crashed or hung worker tasks).

use super::ctx::ApiCtx;
use nv_common::{
    db::entities::package_sources,
    npm::elaboration::{
        ClaimedElaboration, ElaborationLimits, elaborate,
        lifecycle::{
            FailureKind, attempt_is_active, claim_next_due, lease_duration, recover_expired_leases,
        },
    },
};
use sea_orm::DatabaseConnection;
use std::{sync::Arc, time::Duration};
use tokio::task::{JoinHandle, JoinSet};

pub(super) struct Worker(JoinHandle<()>);

impl Drop for Worker {
    fn drop(&mut self) {
        self.0.abort();
    }
}

pub(super) fn spawn(ctx: &ApiCtx, log: slog::Logger) -> Worker {
    let db = ctx.db().clone();
    let registry_url = ctx.npm_registry_url().clone();
    let limits = ctx.package_elaboration_limits();
    let max_bytes = ctx.package_elaboration_max_packument_bytes();
    let admission = ctx.package_elaboration_admission();
    Worker(tokio::spawn(async move {
        let mut tasks = JoinSet::new();
        let mut ticks = tokio::time::interval(Duration::from_secs(1));
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = ticks.tick() => {
                    if recover_expired_leases(&db, 32, &log).await.is_err() {
                        slog::warn!(log, "package-source lease recovery could not access durable state");
                        continue;
                    }
                    // Every dispatcher shares this process-wide resolution
                    // bound, including manually-triggered CLI attempts.
                    for _ in 0..32 {
                        let Some(permit) = admission.try_acquire() else { break };
                        let claim = match claim_next_due(&db, lease_duration(limits.total_run_timeout)).await {
                            Ok(Some(claim)) => claim,
                            Ok(None) => break,
                            Err(_) => {
                                slog::warn!(log, "package-source dispatch could not access durable state");
                                break;
                            }
                        };
                        let db = db.clone();
                        let registry_url = registry_url.clone();
                        let limits = limits.clone();
                        let log = log.clone();
                        tasks.spawn(async move {
                            let _permit = permit;
                            let source_id = claim.id;
                            if run_claim(&db, &claim, registry_url, limits, max_bytes).await.is_err() {
                                slog::error!(log, "package-source attempt awaits lease recovery";
                                    "source_id" => source_id);
                            }
                        });
                    }
                }
                result = tasks.join_next(), if !tasks.is_empty() => {
                    if result.is_some_and(|result| result.is_err()) {
                        slog::error!(log, "package-source worker task stopped; lease recovery will resume its work");
                    }
                }
            }
        }
    }))
}

async fn run_claim(
    db: &DatabaseConnection,
    claim: &package_sources::Model,
    registry_url: url::Url,
    limits: ElaborationLimits,
    max_bytes: usize,
) -> Result<(), &'static str> {
    // Keep worker setup and finalization identical to manual CLI attempts.
    let attempt = ClaimedElaboration::new(db, claim.id, claim.attempt_generation);
    let setup = ClaimedElaboration::prepare(
        claim.file_contents.as_bytes(),
        registry_url,
        &limits,
        max_bytes,
    )
    .map_err(|error| error.failure_kind());
    let result = match setup {
        Ok((source, client)) => {
            let elaboration = elaborate(&source, Arc::new(client), limits);
            tokio::pin!(elaboration);
            tokio::select! {
                result = &mut elaboration => result.map_err(|error| FailureKind::from(&error)),
                () = wait_for_inactive_attempt(db, claim.id, claim.attempt_generation) => {
                    return Ok(());
                }
            }
        }
        Err(kind) => Err(kind),
    };
    attempt
        .finalize(result)
        .await
        .map(|_| ())
        .map_err(|_| "internal")
}

/// Races elaboration against cancellation so a running worker observes it at
/// a bounded safe point instead of only after publication is attempted.
async fn wait_for_inactive_attempt(
    db: &DatabaseConnection,
    source_id: i32,
    attempt_generation: i32,
) {
    loop {
        tokio::time::sleep(Duration::from_millis(250)).await;
        match attempt_is_active(db, source_id, attempt_generation).await {
            Ok(true) | Err(_) => {}
            Ok(false) => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nv_common::db::entities::package_sources;
    use sea_orm::{
        ActiveModelTrait as _, ActiveValue::Set, ConnectionTrait as _, Database,
        DatabaseConnection, DbBackend, EntityTrait as _, MockDatabase, Statement,
    };

    async fn insert(db: &DatabaseConnection, contents: &str) -> package_sources::Model {
        package_sources::ActiveModel {
            source_id: Set(uuid::Uuid::now_v7().to_string()),
            display_name: Set("test package source".to_owned()),
            file_name: Set("package.json".to_owned()),
            file_contents: Set(contents.to_owned()),
            inferred_type: Set("npm-package-json".to_owned()),
            resolution_status: Set("pending".to_owned()),
            ..Default::default()
        }
        .insert(db)
        .await
        .unwrap()
    }

    #[tokio::test]
    #[ignore = "requires NV_LIFECYCLE_TEST_DATABASE_URL and disposable migrated PostgreSQL"]
    async fn dispatcher_resumes_persisted_work_and_classifies_failures() {
        use nv_common::npm::elaboration::lifecycle::{AttemptKind, begin_attempt};
        use std::time::Duration as StdDuration;

        // The target database must already be migrated (this crate has no
        // dependency on `migration`); `nv-common`'s own lifecycle test suite
        // exercises the migration itself.
        let db = Database::connect(std::env::var("NV_LIFECYCLE_TEST_DATABASE_URL").unwrap())
            .await
            .unwrap();
        let abandoned = insert(&db, "{}").await;
        begin_attempt(
            &db,
            abandoned.id,
            AttemptKind::Initial,
            StdDuration::from_mins(1),
        )
        .await
        .unwrap()
        .unwrap();
        db.execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "UPDATE package_sources SET lease_expires_at = clock_timestamp() - interval '1 second' WHERE id = $1",
            [abandoned.id.into()],
        ))
        .await
        .unwrap();
        let invalid = insert(&db, "invalid private source contents").await;
        let context = ApiCtx::for_test(db.clone(), None);
        let worker = spawn(&context, slog::Logger::root(slog::Discard, slog::o!()));
        tokio::time::timeout(StdDuration::from_secs(10), async {
            loop {
                let completed = package_sources::Entity::find_by_id(abandoned.id)
                    .one(&db)
                    .await
                    .unwrap()
                    .unwrap();
                let failed = package_sources::Entity::find_by_id(invalid.id)
                    .one(&db)
                    .await
                    .unwrap()
                    .unwrap();
                if completed.resolution_status == "completed"
                    && failed.resolution_status == "failed"
                {
                    assert_eq!(failed.failure_kind.as_deref(), Some("validation"));
                    assert!(!failed.retryable);
                    break;
                }
                tokio::time::sleep(StdDuration::from_millis(50)).await;
            }
        })
        .await
        .expect("persisted work should finish without another submission");
        drop(worker);

        // Prolonged database failure returns a controlled marker, leaving the
        // existing lease as the recovery mechanism.
        let unavailable = MockDatabase::new(DbBackend::Postgres)
            .append_query_errors([sea_orm::DbErr::Custom(
                "private database details".to_owned(),
            )])
            .into_connection();
        let attempt = ClaimedElaboration::new(&unavailable, 1, 1);
        attempt
            .finalize(Err(FailureKind::Internal))
            .await
            .unwrap_err();
    }
}
