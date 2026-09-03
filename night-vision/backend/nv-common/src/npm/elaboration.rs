//! Concurrent elaboration of NPM package sources.
//!
//! An elaboration run expands every registry-resolvable dependency in a
//! validated [`NpmPackageJson`] into concrete package versions. The supervisor
//! owns the result; workers only inspect one concrete package version and send
//! one report through the one-shot channel attached to their work item.

pub mod storage;

use super::{
    package_json::{DependencyKind, NpmPackageJson},
    packument::{
        NpmPackument, NpmVersion, PackumentBundleDependencies, PackumentDependencyMap,
        PackumentParseError,
    },
    types::{DependencyPackageName, DependencySpec, NpmPackageName},
};
use crate::npm_semver::{NpmVersion as RangeVersion, elaborate_npm_version_bounds, parse_range};
use async_channel::Receiver;
use async_trait::async_trait;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use semver::Version;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::Arc,
    time::Duration,
};
use thiserror::Error;
use tokio::sync::{Mutex, OnceCell, oneshot};
use url::Url;

/// A concrete NPM package identity.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PackageVersion {
    pub name: Box<str>,
    pub version: Box<str>,
}

impl PackageVersion {
    fn from_npm(name: &NpmPackageName, version: impl std::fmt::Display) -> Self {
        Self {
            name: name.as_str().into(),
            version: version.to_string().into_boxed_str(),
        }
    }

    /// Returns this package version's NPM Package URL.
    pub fn purl(&self) -> String {
        match self
            .name
            .strip_prefix('@')
            .and_then(|name| name.split_once('/'))
        {
            Some((scope, package)) => format!("pkg:npm/%40{scope}/{package}@{}", self.version),
            None => format!("pkg:npm/{}@{}", self.name, self.version),
        }
    }
}

/// A nonfatal dependency specification that v1 cannot resolve through NPM.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ElaborationWarning {
    pub declared_by: Option<PackageVersion>,
    pub dependency_name: Box<str>,
    pub specification_kind: UnsupportedSpecificationKind,
}

/// The unsupported source form encountered while elaborating a dependency.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnsupportedSpecificationKind {
    File,
    Git,
    Url,
}

impl UnsupportedSpecificationKind {
    /// Returns a fixed explanation that is safe to expose to API and CLI callers.
    pub fn safe_message(self) -> &'static str {
        match self {
            Self::File => {
                "File, link, and workspace dependencies cannot be resolved through the configured NPM registry."
            }
            Self::Git => {
                "Git and repository dependencies cannot be resolved through the configured NPM registry."
            }
            Self::Url => {
                "Direct URL dependencies cannot be resolved through the configured NPM registry."
            }
        }
    }
}

/// A package version and all known acyclic derivations reaching it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ElaboratedPackage {
    pub package: PackageVersion,
    pub derivations: Vec<Vec<PackageVersion>>,
    /// The resolved source repository for this concrete package version.
    pub source_repository: Option<String>,
}

/// A normalized dependency edge in an elaborated package source.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ElaborationEdge {
    pub parent: Option<PackageVersion>,
    pub child: PackageVersion,
    pub root_dependency_kind: Option<DependencyKind>,
    pub declared_dependency: Box<str>,
    pub declared_specification: Box<str>,
}

/// The complete, in-memory result of elaborating one package source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ElaborationResult {
    pub packages: Vec<ElaboratedPackage>,
    pub edges: Vec<ElaborationEdge>,
    pub warnings: Vec<ElaborationWarning>,
}

/// Resource limits for a single elaboration run.
#[derive(Clone, Debug)]
pub struct ElaborationLimits {
    pub worker_concurrency: usize,
    pub work_queue_capacity: usize,
    pub request_timeout: Duration,
    pub total_run_timeout: Duration,
    pub max_packages: usize,
    pub max_edges: usize,
    pub max_queued_work: usize,
    pub max_derivations: usize,
}

impl Default for ElaborationLimits {
    fn default() -> Self {
        Self {
            worker_concurrency: 4,
            work_queue_capacity: 16,
            request_timeout: Duration::from_secs(30),
            total_run_timeout: Duration::from_mins(5),
            max_packages: 10_000,
            max_edges: 100_000,
            max_queued_work: 100_000,
            max_derivations: 100_000,
        }
    }
}

impl ElaborationLimits {
    fn validate(&self) -> Result<(), ElaborationError> {
        if self.worker_concurrency == 0
            || self.work_queue_capacity == 0
            || self.request_timeout.is_zero()
            || self.total_run_timeout.is_zero()
            || self.max_edges == 0
            || self.max_queued_work == 0
        {
            return Err(ElaborationError::InvalidLimits);
        }
        Ok(())
    }
}

/// Retrieves validated packuments from the configured registry.
#[async_trait]
pub trait PackumentProvider: Send + Sync {
    async fn fetch(&self, package: &NpmPackageName)
    -> Result<NpmPackument, PackumentProviderError>;
}

/// HTTP client for the configured NPM registry.
pub struct NpmRegistryClient {
    client: reqwest::Client,
    registry_url: Url,
    max_packument_bytes: usize,
    request_timeout: Duration,
}

impl NpmRegistryClient {
    /// Creates a registry client with a validated base URL and response limit.
    pub fn new(
        registry_url: Url,
        max_packument_bytes: usize,
        request_timeout: Duration,
    ) -> Result<Self, PackumentProviderError> {
        if max_packument_bytes == 0 || request_timeout.is_zero() {
            return Err(PackumentProviderError::Request);
        }
        Ok(Self {
            client: reqwest::Client::new(),
            registry_url,
            max_packument_bytes,
            request_timeout,
        })
    }

    /// Creates a client for the public NPM registry.
    pub fn public_npm(
        max_packument_bytes: usize,
        request_timeout: Duration,
    ) -> Result<Self, PackumentProviderError> {
        Self::new(
            Url::parse("https://registry.npmjs.org/").expect("public NPM URL is valid"),
            max_packument_bytes,
            request_timeout,
        )
    }

    fn packument_url(&self, package: &NpmPackageName) -> Result<Url, PackumentProviderError> {
        let encoded = utf8_percent_encode(package.as_str(), NON_ALPHANUMERIC).to_string();
        self.registry_url
            .join(&encoded)
            .map_err(|_| PackumentProviderError::Request)
    }
}

#[async_trait]
impl PackumentProvider for NpmRegistryClient {
    async fn fetch(
        &self,
        package: &NpmPackageName,
    ) -> Result<NpmPackument, PackumentProviderError> {
        let bytes = tokio::time::timeout(self.request_timeout, async {
            let response = self
                .client
                .get(self.packument_url(package)?)
                .header("Accept", "application/vnd.npm.install-v1+json")
                .send()
                .await
                .map_err(|_| PackumentProviderError::Request)?;
            if response.status() == reqwest::StatusCode::NOT_FOUND {
                return Err(PackumentProviderError::NotFound);
            }
            if !response.status().is_success()
                || response
                    .content_length()
                    .is_some_and(|length| length > self.max_packument_bytes as u64)
            {
                return Err(PackumentProviderError::Request);
            }
            response
                .bytes()
                .await
                .map_err(|_| PackumentProviderError::Request)
        })
        .await
        .map_err(|_| PackumentProviderError::Timeout)??;
        if bytes.len() > self.max_packument_bytes {
            return Err(PackumentProviderError::Request);
        }
        super::packument::parse_packument(bytes.as_ref())
            .map_err(PackumentProviderError::InvalidPackument)
    }
}

/// A registry access failure safe to return to callers.
#[derive(Debug, Error)]
pub enum PackumentProviderError {
    #[error("packument was not found")]
    NotFound,
    #[error("registry request failed")]
    Request,
    #[error("registry request timed out")]
    Timeout,
    #[error("registry returned an invalid packument")]
    InvalidPackument(#[source] PackumentParseError),
}

/// An elaboration failure. Failures never produce a partial result.
#[derive(Debug, Error)]
pub enum ElaborationError {
    #[error("elaboration limits must be positive")]
    InvalidLimits,
    #[error("failed to retrieve packument for {package}")]
    Packument {
        package: Box<str>,
        #[source]
        source: PackumentProviderError,
    },
    #[error("invalid NPM range for {package}")]
    InvalidRange { package: Box<str> },
    #[error("NPM tag {tag:?} does not exist for {package}")]
    UnknownTag { package: Box<str>, tag: Box<str> },
    #[error("package version {package}@{version} is no longer available")]
    MissingVersion {
        package: Box<str>,
        version: Box<str>,
    },
    #[error("elaboration exceeded the package limit")]
    PackageLimitExceeded,
    #[error("elaboration exceeded the dependency-edge limit")]
    EdgeLimitExceeded,
    #[error("elaboration exceeded the queued-work limit")]
    QueuedWorkLimitExceeded,
    #[error("elaboration exceeded the derivation-path limit")]
    DerivationLimitExceeded,
    #[error("elaboration exceeded the total-run timeout")]
    TotalRunTimeout,
    #[error("a worker stopped before reporting its result")]
    WorkerStopped,
}

/// Expand every NPM package version reachable from `source`.
pub async fn elaborate(
    source: &NpmPackageJson,
    provider: Arc<dyn PackumentProvider>,
    limits: ElaborationLimits,
) -> Result<ElaborationResult, ElaborationError> {
    limits.validate()?;
    let cache = PackumentCache::default();
    let mut workers = WorkerPool::new(
        limits.worker_concurrency,
        limits.work_queue_capacity,
        cache.clone(),
        provider.clone(),
    );
    let result = tokio::time::timeout(
        limits.total_run_timeout,
        elaborate_inner(source, provider, &limits, &cache, workers.sender()),
    )
    .await
    .map_err(|_| ElaborationError::TotalRunTimeout)
    .and_then(|result| result);
    match result {
        Ok(result) => {
            workers.shutdown().await?;
            Ok(result)
        }
        Err(error) => {
            workers.abort();
            Err(error)
        }
    }
}

async fn elaborate_inner(
    source: &NpmPackageJson,
    provider: Arc<dyn PackumentProvider>,
    limits: &ElaborationLimits,
    cache: &PackumentCache,
    work_sender: &async_channel::Sender<WorkItem>,
) -> Result<ElaborationResult, ElaborationError> {
    let mut state = SupervisorState::default();
    let mut pending = Vec::new();
    for root in source.root_dependencies() {
        let versions = resolve_specification(
            cache,
            provider.as_ref(),
            &root.name,
            &root.specification,
            None,
            &mut state.warnings,
        )
        .await?;
        for version in versions {
            state.record_edge(
                ElaborationEdge {
                    parent: None,
                    child: version.clone(),
                    root_dependency_kind: Some(root.kind),
                    declared_dependency: root.name.as_str().into(),
                    declared_specification: format_dependency_specification(&root.specification),
                },
                limits,
            )?;
            enqueue_work(&mut pending, (version.clone(), vec![version]), limits)?;
        }
    }

    while !pending.is_empty() {
        let mut replies = Vec::new();
        let mut work = BTreeMap::<PackageVersion, Vec<Vec<PackageVersion>>>::new();
        while let Some((package, derivation)) = pending.pop() {
            if !state.record_derivation(package.clone(), derivation.clone(), limits)? {
                continue;
            }
            work.entry(package).or_default().push(derivation);
        }

        for (package, derivations) in work {
            let (reply_sender, reply_receiver) = oneshot::channel();
            work_sender
                .send(WorkItem {
                    package,
                    derivations,
                    reply_sender,
                })
                .await
                .map_err(|_| ElaborationError::WorkerStopped)?;
            replies.push(reply_receiver);
        }

        for reply in replies {
            let report = reply.await.map_err(|_| ElaborationError::WorkerStopped)??;
            state
                .repositories
                .insert(report.package.clone(), report.source_repository.clone());
            for dependency in report.dependencies {
                let versions = resolve_specification(
                    cache,
                    provider.as_ref(),
                    &dependency.name,
                    &dependency.specification,
                    Some(&report.package),
                    &mut state.warnings,
                )
                .await?;
                for derivation in &report.derivations {
                    for version in &versions {
                        state.record_edge(
                            ElaborationEdge {
                                parent: Some(report.package.clone()),
                                child: version.clone(),
                                root_dependency_kind: None,
                                declared_dependency: dependency.name.as_str().into(),
                                declared_specification: format_dependency_specification(
                                    &dependency.specification,
                                ),
                            },
                            limits,
                        )?;
                        if derivation.contains(version) {
                            continue;
                        }
                        let mut child_derivation = derivation.clone();
                        child_derivation.push(version.clone());
                        enqueue_work(&mut pending, (version.clone(), child_derivation), limits)?;
                    }
                }
            }
        }
    }

    Ok(state.finish())
}

fn enqueue_work(
    pending: &mut Vec<(PackageVersion, Vec<PackageVersion>)>,
    work: (PackageVersion, Vec<PackageVersion>),
    limits: &ElaborationLimits,
) -> Result<(), ElaborationError> {
    if pending.len() == limits.max_queued_work {
        return Err(ElaborationError::QueuedWorkLimitExceeded);
    }
    pending.push(work);
    Ok(())
}

#[derive(Default)]
struct PackumentCache {
    entries: Arc<Mutex<HashMap<PkgName, CachedNpmPackument>>>,
}

type PkgName = Box<str>;
type CachedNpmPackument = Arc<OnceCell<Arc<NpmPackument>>>;

impl Clone for PackumentCache {
    fn clone(&self) -> Self {
        Self {
            entries: self.entries.clone(),
        }
    }
}

impl PackumentCache {
    async fn get(
        &self,
        provider: &dyn PackumentProvider,
        package: &NpmPackageName,
    ) -> Result<Arc<NpmPackument>, ElaborationError> {
        let cell = {
            let mut entries = self.entries.lock().await;
            entries
                .entry(package.as_str().into())
                .or_insert_with(|| Arc::new(OnceCell::new()))
                .clone()
        };
        let package_name = package.as_str().to_owned();
        cell.get_or_try_init(|| async {
            provider
                .fetch(package)
                .await
                .map(Arc::new)
                .map_err(|source| ElaborationError::Packument {
                    package: package_name.into_boxed_str(),
                    source,
                })
        })
        .await
        .cloned()
    }
}

struct WorkItem {
    package: PackageVersion,
    derivations: Vec<Vec<PackageVersion>>,
    reply_sender: oneshot::Sender<Result<WorkerReport, ElaborationError>>,
}

struct WorkerPool {
    sender: async_channel::Sender<WorkItem>,
    workers: Vec<tokio::task::JoinHandle<()>>,
}

impl WorkerPool {
    fn new(
        concurrency: usize,
        capacity: usize,
        cache: PackumentCache,
        provider: Arc<dyn PackumentProvider>,
    ) -> Self {
        let (sender, receiver) = async_channel::bounded(capacity);
        let workers = (0..concurrency)
            .map(|_| tokio::spawn(worker(receiver.clone(), cache.clone(), provider.clone())))
            .collect();
        Self { sender, workers }
    }

    fn sender(&self) -> &async_channel::Sender<WorkItem> {
        &self.sender
    }

    async fn shutdown(&mut self) -> Result<(), ElaborationError> {
        self.sender.close();
        for worker in self.workers.drain(..) {
            worker.await.map_err(|_| ElaborationError::WorkerStopped)?;
        }
        Ok(())
    }

    fn abort(&mut self) {
        self.sender.close();
        for worker in self.workers.drain(..) {
            worker.abort();
        }
    }
}

impl Drop for WorkerPool {
    fn drop(&mut self) {
        self.abort();
    }
}

struct WorkerReport {
    package: PackageVersion,
    derivations: Vec<Vec<PackageVersion>>,
    dependencies: Vec<DeclaredDependency>,
    source_repository: Option<String>,
}

struct DeclaredDependency {
    name: DependencyPackageName,
    specification: DependencySpec,
}

async fn worker(
    receiver: Receiver<WorkItem>,
    cache: PackumentCache,
    provider: Arc<dyn PackumentProvider>,
) {
    while let Ok(item) = receiver.recv().await {
        let result = inspect_package(
            item.package.clone(),
            item.derivations,
            &cache,
            provider.as_ref(),
        )
        .await;
        let _ = item.reply_sender.send(result);
    }
}

async fn inspect_package(
    package: PackageVersion,
    derivations: Vec<Vec<PackageVersion>>,
    cache: &PackumentCache,
    provider: &dyn PackumentProvider,
) -> Result<WorkerReport, ElaborationError> {
    let name = NpmPackageName::parse(package.name.to_string())
        .expect("package names originate in packuments");
    let packument = cache.get(provider, &name).await?;
    let version = Version::parse(&package.version).expect("versions originate in packuments");
    let npm_version =
        packument
            .versions
            .get(&version)
            .ok_or_else(|| ElaborationError::MissingVersion {
                package: package.name.clone(),
                version: package.version.clone(),
            })?;
    Ok(WorkerReport {
        package,
        derivations,
        dependencies: declared_dependencies(npm_version),
        source_repository: npm_version
            .repository
            .as_ref()
            .or(packument.repository.as_ref())
            .and_then(|repository| normalize_repository_url(&repository.url)),
    })
}

fn declared_dependencies(version: &NpmVersion) -> Vec<DeclaredDependency> {
    let mut dependencies = Vec::new();
    append_dependencies(&mut dependencies, &version.dependencies);
    append_dependencies(&mut dependencies, &version.dev_dependencies);
    append_dependencies(&mut dependencies, &version.peer_dependencies);
    append_dependencies(&mut dependencies, &version.optional_dependencies);
    append_bundled_dependencies(
        &mut dependencies,
        &version.bundle_dependencies,
        &version.dependencies,
    );
    dependencies.sort_by(|left, right| left.name.as_str().cmp(right.name.as_str()));
    dependencies.dedup_by(|left, right| {
        left.name == right.name && left.specification == right.specification
    });
    dependencies
}

fn append_dependencies(
    output: &mut Vec<DeclaredDependency>,
    dependencies: &PackumentDependencyMap,
) {
    output.extend(dependencies.iter().filter_map(|(name, specification)| {
        Some(DeclaredDependency {
            name: name.valid()?.clone(),
            specification: specification.clone(),
        })
    }));
}

fn append_bundled_dependencies(
    output: &mut Vec<DeclaredDependency>,
    bundled: &PackumentBundleDependencies,
    dependencies: &PackumentDependencyMap,
) {
    output.extend(bundled.as_slice().iter().filter_map(|name| {
        let name = name.valid()?;
        dependencies
            .iter()
            .find_map(|(dependency_name, specification)| {
                (dependency_name.valid() == Some(name)).then_some(specification)
            })
            .map(|specification| DeclaredDependency {
                name: name.clone(),
                specification: specification.clone(),
            })
    }));
}

fn format_dependency_specification(specification: &DependencySpec) -> Box<str> {
    match specification {
        DependencySpec::Registry(range) => range.as_str().into(),
        DependencySpec::Tag(tag) | DependencySpec::File(tag) | DependencySpec::Git(tag) => {
            tag.clone().into_boxed_str()
        }
        DependencySpec::Url(url) => url.as_str().into(),
        DependencySpec::NpmAlias {
            package,
            specification,
        } => format!(
            "npm:{}@{}",
            package.as_str(),
            format_dependency_specification(specification)
        )
        .into_boxed_str(),
    }
}

async fn resolve_specification(
    cache: &PackumentCache,
    provider: &dyn PackumentProvider,
    name: &DependencyPackageName,
    specification: &DependencySpec,
    declared_by: Option<&PackageVersion>,
    warnings: &mut Vec<ElaborationWarning>,
) -> Result<Vec<PackageVersion>, ElaborationError> {
    let mut name = name;
    let mut specification = specification;
    while let DependencySpec::NpmAlias {
        package,
        specification: target_specification,
    } = specification
    {
        name = package;
        specification = target_specification;
    }

    match specification {
        DependencySpec::NpmAlias { .. } => unreachable!("aliases were unwrapped above"),
        DependencySpec::File(_) => {
            warnings.push(ElaborationWarning {
                declared_by: declared_by.cloned(),
                dependency_name: name.as_str().into(),
                specification_kind: UnsupportedSpecificationKind::File,
            });
            Ok(Vec::new())
        }
        DependencySpec::Git(_) => {
            warnings.push(ElaborationWarning {
                declared_by: declared_by.cloned(),
                dependency_name: name.as_str().into(),
                specification_kind: UnsupportedSpecificationKind::Git,
            });
            Ok(Vec::new())
        }
        DependencySpec::Url(_) => {
            warnings.push(ElaborationWarning {
                declared_by: declared_by.cloned(),
                dependency_name: name.as_str().into(),
                specification_kind: UnsupportedSpecificationKind::Url,
            });
            Ok(Vec::new())
        }
        DependencySpec::Registry(_) | DependencySpec::Tag(_) => {
            let package = NpmPackageName::parse(name.as_str().to_owned()).map_err(|_| {
                ElaborationError::InvalidRange {
                    package: name.as_str().into(),
                }
            })?;
            let packument = cache.get(provider, &package).await?;
            match specification {
                DependencySpec::Registry(raw_range) => {
                    let range = parse_range(raw_range.as_str()).map_err(|_| {
                        ElaborationError::InvalidRange {
                            package: package.as_str().into(),
                        }
                    })?;
                    let mut versions = packument
                        .versions
                        .keys()
                        .map(|version| {
                            RangeVersion::parse(version.to_string())
                                .expect("packument versions are valid SemVer")
                        })
                        .collect::<Vec<_>>();
                    versions.sort();
                    Ok(elaborate_npm_version_bounds(&versions, &range)
                        .expect("versions were sorted")
                        .into_iter()
                        .map(|version| PackageVersion::from_npm(&packument.name, version))
                        .collect())
                }
                DependencySpec::Tag(tag) => packument
                    .dist_tags
                    .get(tag)
                    .map(|version| vec![PackageVersion::from_npm(&packument.name, version)])
                    .ok_or_else(|| ElaborationError::UnknownTag {
                        package: package.as_str().into(),
                        tag: tag.clone().into_boxed_str(),
                    }),
                _ => unreachable!("the outer match constrains specification"),
            }
        }
    }
}

#[derive(Default)]
struct SupervisorState {
    packages: BTreeMap<PackageVersion, BTreeSet<Vec<PackageVersion>>>,
    edges: BTreeSet<ElaborationEdge>,
    warnings: Vec<ElaborationWarning>,
    repositories: BTreeMap<PackageVersion, Option<String>>,
}

impl SupervisorState {
    fn record_edge(
        &mut self,
        edge: ElaborationEdge,
        limits: &ElaborationLimits,
    ) -> Result<(), ElaborationError> {
        if self.edges.contains(&edge) {
            return Ok(());
        }
        if self.edges.len() == limits.max_edges {
            return Err(ElaborationError::EdgeLimitExceeded);
        }
        self.edges.insert(edge);
        Ok(())
    }

    fn record_derivation(
        &mut self,
        package: PackageVersion,
        derivation: Vec<PackageVersion>,
        limits: &ElaborationLimits,
    ) -> Result<bool, ElaborationError> {
        if !self.packages.contains_key(&package) && self.packages.len() == limits.max_packages {
            return Err(ElaborationError::PackageLimitExceeded);
        }
        if self
            .packages
            .get(&package)
            .is_some_and(|paths| paths.contains(&derivation))
        {
            return Ok(false);
        }
        let count = self.packages.values().map(BTreeSet::len).sum::<usize>();
        if count == limits.max_derivations {
            return Err(ElaborationError::DerivationLimitExceeded);
        }
        self.packages.entry(package).or_default().insert(derivation);
        Ok(true)
    }

    fn finish(mut self) -> ElaborationResult {
        self.warnings.sort_by(|left, right| {
            left.declared_by
                .cmp(&right.declared_by)
                .then(left.dependency_name.cmp(&right.dependency_name))
        });
        ElaborationResult {
            packages: self
                .packages
                .into_iter()
                .map(|(package, derivations)| ElaboratedPackage {
                    source_repository: self.repositories.remove(&package).flatten(),
                    package,
                    derivations: derivations.into_iter().collect(),
                })
                .collect(),
            edges: self.edges.into_iter().collect(),
            warnings: self.warnings,
        }
    }
}

fn normalize_repository_url(value: &str) -> Option<String> {
    let value = value.strip_prefix("git+").unwrap_or(value);
    let url = Url::parse(value).ok()?;
    matches!(url.scheme(), "http" | "https").then(|| url.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::npm::{package_json::NpmPackageJson, packument::parse_packument};
    use httpmock::prelude::*;
    use serde_json::json;
    use std::{
        collections::BTreeMap,
        error::Error as _,
        sync::atomic::{AtomicUsize, Ordering},
        time::Duration,
    };

    struct MockRegistry {
        packuments: BTreeMap<String, String>,
    }

    #[async_trait]
    impl PackumentProvider for MockRegistry {
        async fn fetch(
            &self,
            package: &NpmPackageName,
        ) -> Result<NpmPackument, PackumentProviderError> {
            let Some(packument) = self.packuments.get(package.as_str()) else {
                return Err(PackumentProviderError::NotFound);
            };
            parse_packument(packument.as_bytes()).map_err(PackumentProviderError::InvalidPackument)
        }
    }

    fn packument(name: &str, version: &str, dependencies: serde_json::Value) -> String {
        json!({
            "name": name,
            "dist-tags": { "latest": version },
            "versions": {
                version: {
                    "name": name,
                    "version": version,
                    "dependencies": dependencies,
                    "dist": {
                        "tarball": format!("https://registry.example/{name}-{version}.tgz"),
                        "shasum": "0123456789012345678901234567890123456789"
                    }
                }
            }
        })
        .to_string()
    }

    fn source(value: serde_json::Value) -> NpmPackageJson {
        NpmPackageJson::parse_package_json(value.to_string().as_bytes()).expect("valid manifest")
    }

    fn run(
        source: &NpmPackageJson,
        registry: MockRegistry,
    ) -> Result<ElaborationResult, ElaborationError> {
        tokio::runtime::Runtime::new()
            .expect("runtime")
            .block_on(elaborate(
                source,
                Arc::new(registry),
                ElaborationLimits::default(),
            ))
    }

    #[test]
    fn elaborates_ranges_and_transitive_dependencies() {
        let source = source(json!({ "dependencies": { "root": "^1.0.0" } }));
        let registry = MockRegistry {
            packuments: BTreeMap::from([
                (
                    "root".to_owned(),
                    packument("root", "1.0.0", json!({ "child": "^2.0.0" })),
                ),
                ("child".to_owned(), packument("child", "2.0.0", json!({}))),
            ]),
        };

        let result = run(&source, registry).expect("elaboration succeeds");

        assert_eq!(result.packages.len(), 2);
        assert_eq!(result.edges.len(), 2);
        assert!(result.edges.iter().any(|edge| {
            edge.parent.is_none()
                && edge.child.purl() == "pkg:npm/root@1.0.0"
                && edge.root_dependency_kind == Some(DependencyKind::Dependencies)
                && edge.declared_specification.as_ref() == "^1.0.0"
        }));
        assert!(result.edges.iter().any(|edge| {
            edge.parent
                .as_ref()
                .is_some_and(|parent| parent.purl() == "pkg:npm/root@1.0.0")
                && edge.child.purl() == "pkg:npm/child@2.0.0"
                && edge.declared_dependency.as_ref() == "child"
        }));
        assert_eq!(result.packages[0].package.purl(), "pkg:npm/child@2.0.0");
        assert_eq!(result.packages[1].package.purl(), "pkg:npm/root@1.0.0");
        assert_eq!(
            result.packages[0].derivations[0]
                .iter()
                .map(PackageVersion::purl)
                .collect::<Vec<_>>(),
            ["pkg:npm/root@1.0.0", "pkg:npm/child@2.0.0"]
        );
    }

    #[test]
    fn records_non_registry_specifications_as_warnings() {
        let source = source(json!({ "dependencies": { "local": "file:../local" } }));

        let result = run(
            &source,
            MockRegistry {
                packuments: BTreeMap::new(),
            },
        )
        .expect("unsupported dependency is nonfatal");

        assert!(result.packages.is_empty());
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(
            result.warnings[0].specification_kind,
            UnsupportedSpecificationKind::File
        );
        assert_eq!(
            result.warnings[0].specification_kind.safe_message(),
            "File, link, and workspace dependencies cannot be resolved through the configured NPM registry."
        );
    }

    #[test]
    fn ignores_invalid_packument_dependency_names() {
        let source = source(json!({ "dependencies": { "root": "1.0.0" } }));
        let registry = MockRegistry {
            packuments: BTreeMap::from([(
                "root".to_owned(),
                packument("root", "1.0.0", json!({ "equire('express'": "*" })),
            )]),
        };

        let result = run(&source, registry).expect("invalid upstream name is not resolved");

        assert_eq!(result.packages.len(), 1);
        assert!(
            result
                .packages
                .iter()
                .all(|package| package.package.name.as_ref() != "equire('express'")
        );
    }

    struct FailingRegistry;

    #[async_trait]
    impl PackumentProvider for FailingRegistry {
        async fn fetch(
            &self,
            _package: &NpmPackageName,
        ) -> Result<NpmPackument, PackumentProviderError> {
            Err(PackumentProviderError::Request)
        }
    }

    #[test]
    fn reports_mocked_registry_failures() {
        let source = source(json!({ "dependencies": { "root": "1.0.0" } }));

        let error = tokio::runtime::Runtime::new()
            .expect("runtime")
            .block_on(elaborate(
                &source,
                Arc::new(FailingRegistry),
                ElaborationLimits::default(),
            ))
            .expect_err("registry failure is reported");

        assert!(matches!(
            error,
            ElaborationError::Packument {
                source: PackumentProviderError::Request,
                ..
            }
        ));
    }

    #[test]
    fn reports_the_packument_parse_error() {
        let source = source(json!({ "dependencies": { "root": "1.0.0" } }));
        let registry = MockRegistry {
            packuments: BTreeMap::from([("root".to_owned(), "{}".to_owned())]),
        };

        let error = run(&source, registry).expect_err("invalid packument is reported");

        assert_eq!(error.to_string(), "failed to retrieve packument for root");
        let provider_error = error.source().expect("provider error is preserved");
        assert_eq!(
            provider_error.to_string(),
            "registry returned an invalid packument"
        );
        assert_eq!(
            provider_error
                .source()
                .expect("packument parse error is preserved")
                .to_string(),
            "packument is missing required field $.name"
        );
    }

    #[test]
    fn records_bundle_dependencies_as_root_edges() {
        let source = source(json!({
            "dependencies": { "bundled": "1.0.0" },
            "bundleDependencies": ["bundled"],
        }));
        let registry = MockRegistry {
            packuments: BTreeMap::from([(
                "bundled".to_owned(),
                packument("bundled", "1.0.0", json!({})),
            )]),
        };

        let result = run(&source, registry).expect("bundle dependency resolves");

        assert!(result.edges.iter().any(|edge| {
            edge.root_dependency_kind == Some(DependencyKind::BundleDependencies)
                && edge.child.purl() == "pkg:npm/bundled@1.0.0"
        }));
    }

    #[test]
    fn stops_expanding_cycles_but_retains_the_reachable_versions() {
        let source = source(json!({ "dependencies": { "left": "1.0.0" } }));
        let registry = MockRegistry {
            packuments: BTreeMap::from([
                (
                    "left".to_owned(),
                    packument("left", "1.0.0", json!({ "right": "1.0.0" })),
                ),
                (
                    "right".to_owned(),
                    packument("right", "1.0.0", json!({ "left": "1.0.0" })),
                ),
            ]),
        };

        let result = run(&source, registry).expect("cycle is finite");

        assert_eq!(result.packages.len(), 2);
        assert_eq!(result.packages[1].derivations.len(), 1);
    }

    #[test]
    fn fails_when_a_limit_is_exceeded() {
        let source = source(json!({ "dependencies": { "root": "1.0.0" } }));
        let registry = MockRegistry {
            packuments: BTreeMap::from([(
                "root".to_owned(),
                packument("root", "1.0.0", json!({})),
            )]),
        };
        let limits = ElaborationLimits {
            max_packages: 0,
            ..ElaborationLimits::default()
        };

        let error = tokio::runtime::Runtime::new()
            .expect("runtime")
            .block_on(elaborate(&source, Arc::new(registry), limits))
            .expect_err("package limit is enforced");

        assert!(matches!(error, ElaborationError::PackageLimitExceeded));
    }

    #[test]
    fn fails_when_the_edge_limit_is_exceeded() {
        let source = source(json!({ "dependencies": { "root": "1.0.0" } }));
        let registry = MockRegistry {
            packuments: BTreeMap::from([
                (
                    "root".to_owned(),
                    packument("root", "1.0.0", json!({ "child": "1.0.0" })),
                ),
                ("child".to_owned(), packument("child", "1.0.0", json!({}))),
            ]),
        };
        let limits = ElaborationLimits {
            max_edges: 1,
            ..ElaborationLimits::default()
        };

        let error = tokio::runtime::Runtime::new()
            .expect("runtime")
            .block_on(elaborate(&source, Arc::new(registry), limits))
            .expect_err("edge limit is enforced");

        assert!(matches!(error, ElaborationError::EdgeLimitExceeded));
    }

    #[test]
    fn fails_when_the_pending_work_limit_is_exceeded() {
        let source = source(json!({
            "dependencies": { "left": "1.0.0", "right": "1.0.0" }
        }));
        let registry = MockRegistry {
            packuments: BTreeMap::from([
                ("left".to_owned(), packument("left", "1.0.0", json!({}))),
                ("right".to_owned(), packument("right", "1.0.0", json!({}))),
            ]),
        };
        let limits = ElaborationLimits {
            max_queued_work: 1,
            ..ElaborationLimits::default()
        };

        let error = tokio::runtime::Runtime::new()
            .expect("runtime")
            .block_on(elaborate(&source, Arc::new(registry), limits))
            .expect_err("queued-work limit is enforced");

        assert!(matches!(error, ElaborationError::QueuedWorkLimitExceeded));
    }

    struct DelayedRegistry;

    #[async_trait]
    impl PackumentProvider for DelayedRegistry {
        async fn fetch(
            &self,
            _package: &NpmPackageName,
        ) -> Result<NpmPackument, PackumentProviderError> {
            tokio::time::sleep(Duration::from_millis(50)).await;
            unreachable!("the total timeout should cancel this fetch")
        }
    }

    #[test]
    fn fails_when_the_total_run_timeout_is_exceeded() {
        let source = source(json!({ "dependencies": { "root": "1.0.0" } }));
        let limits = ElaborationLimits {
            total_run_timeout: Duration::from_millis(1),
            ..ElaborationLimits::default()
        };

        let error = tokio::runtime::Runtime::new()
            .expect("runtime")
            .block_on(elaborate(&source, Arc::new(DelayedRegistry), limits))
            .expect_err("total-run timeout is enforced");

        assert!(matches!(error, ElaborationError::TotalRunTimeout));
    }

    #[tokio::test]
    async fn registry_client_enforces_the_request_timeout() {
        let server = MockServer::start();
        let _mock = server.mock(|when, then| {
            when.method(GET).path("/root");
            then.status(200).delay(Duration::from_millis(50));
        });
        let client = NpmRegistryClient::new(
            Url::parse(&format!("{}/", server.base_url())).expect("valid mock URL"),
            1024,
            Duration::from_millis(1),
        )
        .expect("valid client");
        let package = NpmPackageName::parse("root".to_owned()).expect("valid package name");

        let error = client.fetch(&package).await.expect_err("request times out");

        assert!(matches!(error, PackumentProviderError::Timeout));
    }

    #[test]
    fn resolves_dist_tags_and_npm_aliases() {
        let source = source(json!({
            "dependencies": { "alias": "npm:target@latest" }
        }));
        let registry = MockRegistry {
            packuments: BTreeMap::from([(
                "target".to_owned(),
                packument("target", "1.0.0", json!({})),
            )]),
        };

        let result = run(&source, registry).expect("tagged alias resolves");

        assert_eq!(result.packages.len(), 1);
        assert_eq!(result.packages[0].package.purl(), "pkg:npm/target@1.0.0");
    }

    struct CountingRegistry {
        packument: String,
        requests: AtomicUsize,
    }

    #[async_trait]
    impl PackumentProvider for CountingRegistry {
        async fn fetch(
            &self,
            _package: &NpmPackageName,
        ) -> Result<NpmPackument, PackumentProviderError> {
            self.requests.fetch_add(1, Ordering::SeqCst);
            parse_packument(self.packument.as_bytes())
                .map_err(PackumentProviderError::InvalidPackument)
        }
    }

    #[test]
    fn cache_reuses_a_packument_across_dependency_kinds() {
        let source = source(json!({
            "dependencies": { "shared": "1.0.0" },
            "devDependencies": { "shared": "1.0.0" }
        }));
        let registry = Arc::new(CountingRegistry {
            packument: packument("shared", "1.0.0", json!({})),
            requests: AtomicUsize::new(0),
        });
        let result = tokio::runtime::Runtime::new()
            .expect("runtime")
            .block_on(elaborate(
                &source,
                registry.clone(),
                ElaborationLimits::default(),
            ))
            .expect("elaboration succeeds");

        assert_eq!(result.packages.len(), 1);
        assert_eq!(registry.requests.load(Ordering::SeqCst), 1);
    }
}
