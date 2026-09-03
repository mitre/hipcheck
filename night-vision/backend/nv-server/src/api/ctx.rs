use crate::error::FatalError;
use nv_common::{
    config::Config, hipcheck::HipcheckRunnerConfig, npm::elaboration::ElaborationLimits,
};
use sea_orm::DatabaseConnection;
use std::sync::Arc;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use url::Url;

/// Shared app context, available to every endpoint handler.
///
/// In a request handler, get it with `ctx.context()`.
pub struct ApiCtx {
    /// Handle to the database.
    db: DatabaseConnection,
    npm_registry_url: Url,
    package_elaboration_limits: ElaborationLimits,
    package_elaboration_max_packument_bytes: usize,
    package_elaboration_admission: PackageElaborationAdmission,
    hipcheck_runner_config: HipcheckRunnerConfig,
    hipcheck_admission: PackageElaborationAdmission,
}

impl ApiCtx {
    /// Try to initialize the application context.
    pub async fn init(config: &Config) -> Result<Self, FatalError> {
        let db = nv_common::db::connection(config).await?;
        Ok(Self {
            db,
            npm_registry_url: config.npm_registry_url.clone(),
            package_elaboration_limits: config.package_elaboration_limits(),
            package_elaboration_max_packument_bytes: config.package_elaboration_max_packument_bytes,
            package_elaboration_admission: PackageElaborationAdmission::new(
                config.package_elaboration_max_concurrent_runs,
            ),
            hipcheck_runner_config: config.hipcheck_runner_config(),
            hipcheck_admission: PackageElaborationAdmission::new(
                config.hipcheck_max_concurrent_runs,
            ),
        })
    }

    /// Get a handle to the database.
    pub fn db(&self) -> &DatabaseConnection {
        &self.db
    }

    pub fn npm_registry_url(&self) -> &Url {
        &self.npm_registry_url
    }
    pub fn package_elaboration_limits(&self) -> ElaborationLimits {
        self.package_elaboration_limits.clone()
    }
    pub fn package_elaboration_max_packument_bytes(&self) -> usize {
        self.package_elaboration_max_packument_bytes
    }

    /// Try to reserve a process-wide elaboration run slot.
    ///
    /// The returned permit remains held until the background task exits.
    pub fn try_admit_package_elaboration(&self) -> Option<OwnedSemaphorePermit> {
        self.package_elaboration_admission.try_acquire()
    }
    pub fn hipcheck_runner_config(&self) -> HipcheckRunnerConfig {
        self.hipcheck_runner_config.clone()
    }
    pub fn try_admit_hipcheck(&self) -> Option<OwnedSemaphorePermit> {
        self.hipcheck_admission.try_acquire()
    }
}

#[derive(Clone)]
struct PackageElaborationAdmission(Arc<Semaphore>);

impl PackageElaborationAdmission {
    fn new(max_concurrent_runs: usize) -> Self {
        Self(Arc::new(Semaphore::new(max_concurrent_runs)))
    }

    fn try_acquire(&self) -> Option<OwnedSemaphorePermit> {
        self.0.clone().try_acquire_owned().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::PackageElaborationAdmission;

    #[test]
    fn admission_slot_is_released_when_its_permit_drops() {
        let admission = PackageElaborationAdmission::new(1);
        let permit = admission.try_acquire().expect("first run is admitted");
        assert!(admission.try_acquire().is_none());

        drop(permit);

        assert!(admission.try_acquire().is_some());
    }
}
