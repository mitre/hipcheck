use crate::error::FatalError;
use dropshot::HttpError;
use nv_common::config::Config;
use sea_orm::DatabaseConnection;
use std::{sync::Arc, time::Duration};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// Shared app context, available to every endpoint handler.
///
/// In a request handler, get it with `ctx.context()`.
pub struct ApiCtx {
    /// Handle to the database.
    db: DatabaseConnection,
    package_source_contents_max_bytes: usize,
    package_source_request_timeout: Duration,
    package_source_submission_slots: Arc<Semaphore>,
}

impl ApiCtx {
    /// Try to initialize the application context.
    pub async fn init(config: &Config) -> Result<Self, FatalError> {
        let db = nv_common::db::connection(config).await?;
        Ok(Self {
            db,
            package_source_contents_max_bytes: config.package_source_contents_max_bytes,
            package_source_request_timeout: config.package_source_request_timeout(),
            package_source_submission_slots: Arc::new(Semaphore::new(
                config.package_source_max_concurrency,
            )),
        })
    }

    /// Get a handle to the database.
    pub fn db(&self) -> &DatabaseConnection {
        &self.db
    }

    /// Returns the maximum accepted package-source contents size.
    pub fn package_source_contents_max_bytes(&self) -> usize {
        self.package_source_contents_max_bytes
    }

    /// Returns the timeout for accepting one package-source submission.
    pub fn package_source_request_timeout(&self) -> Duration {
        self.package_source_request_timeout
    }

    /// Acquires capacity for validation and acceptance of a package-source submission.
    pub fn try_acquire_package_source_submission_slot(
        &self,
    ) -> Result<OwnedSemaphorePermit, HttpError> {
        try_acquire_package_source_submission_slot(self.package_source_submission_slots.clone())
    }
}

fn try_acquire_package_source_submission_slot(
    slots: Arc<Semaphore>,
) -> Result<OwnedSemaphorePermit, HttpError> {
    slots.try_acquire_owned().map_err(|_| {
        HttpError::for_unavail(
            Some("PackageSourceCapacity".to_owned()),
            "package-source submission capacity is exhausted".to_owned(),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_source_submission_limit_rejects_excess_work() {
        let slots = Arc::new(Semaphore::new(1));
        let _permit = try_acquire_package_source_submission_slot(slots.clone())
            .expect("the first submission should acquire capacity");

        assert!(try_acquire_package_source_submission_slot(slots).is_err());
    }
}
