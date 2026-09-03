//! Concurrent elaboration of NPM package sources.
//!
//! An elaboration run expands every registry-resolvable dependency in a
//! validated [`NpmPackageJson`] into concrete package versions. The supervisor
//! owns the result; workers only inspect one concrete package version and send
//! one report through the one-shot channel attached to their work item.

pub mod storage;

use super::{
    package_json::{DependencyKind, NpmPackageJson, RootDependency},
    packument::{
        NpmPackument, NpmVersion, PackumentBundleDependencies, PackumentDependencyMap,
        PackumentParseError,
    },
    types::{DependencyPackageName, DependencySpec, NpmPackageName},
};
use crate::npm_semver::{NpmVersion as RangeVersion, elaborate_npm_version_bounds, parse_range};
use async_channel::{Receiver, TrySendError};
use async_trait::async_trait;
use futures_util::{StreamExt as _, stream::FuturesUnordered};
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use reqwest::header::RETRY_AFTER;
use semver::Version;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, VecDeque},
    sync::Arc,
    time::{Duration, SystemTime},
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

/// A point-in-time view of NPM package elaboration work.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ElaborationProgressSnapshot {
    /// Distinct concrete package versions discovered so far.
    pub packages: usize,
    /// Distinct dependency edges discovered so far.
    pub edges: usize,
    /// Distinct acyclic derivation paths discovered so far.
    pub derivations: usize,
    /// Work buffered by the scheduler but not yet dispatched to a worker.
    pub queued_work: usize,
    /// Work dispatched to workers and awaiting a report.
    pub in_flight_work: usize,
    /// Worker reports processed by the scheduler.
    pub completed_work_items: usize,
}

/// A progress event emitted while elaborating an NPM package source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ElaborationProgress {
    /// Elaboration has started.
    Started,
    /// A previously unseen packument is being fetched from the registry.
    PackumentFetchStarted { package: Box<str> },
    /// A packument fetch has completed, whether successfully or with an error.
    PackumentFetchCompleted { package: Box<str> },
    /// The scheduler's known work and result counts have changed.
    SchedulerUpdated {
        snapshot: ElaborationProgressSnapshot,
    },
    /// Elaboration has completed successfully.
    Finished {
        snapshot: ElaborationProgressSnapshot,
    },
}

/// Receives NPM package elaboration progress events.
pub trait ElaborationProgressReporter: Send + Sync {
    /// Reports one progress event.
    fn report(&self, progress: ElaborationProgress);
}

impl<F> ElaborationProgressReporter for F
where
    F: Fn(ElaborationProgress) + Send + Sync,
{
    fn report(&self, progress: ElaborationProgress) {
        self(progress);
    }
}

/// A progress reporter that ignores all events.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoopElaborationProgress;

impl ElaborationProgressReporter for NoopElaborationProgress {
    fn report(&self, _progress: ElaborationProgress) {}
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
const MAX_RATE_LIMIT_RETRIES: usize = 3;
const RATE_LIMIT_BACKOFFS: [Duration; MAX_RATE_LIMIT_RETRIES] = [
    Duration::from_millis(250),
    Duration::from_millis(500),
    Duration::from_secs(1),
];

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
        for retry in 0..=MAX_RATE_LIMIT_RETRIES {
            let attempt = tokio::time::timeout(self.request_timeout, async {
                let response = self
                    .client
                    .get(
                        self.packument_url(package)
                            .map_err(FetchAttemptError::Provider)?,
                    )
                    .header("Accept", "application/vnd.npm.install-v1+json")
                    .send()
                    .await
                    .map_err(map_request_error)
                    .map_err(FetchAttemptError::Provider)?;
                if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
                    return Err(FetchAttemptError::RateLimited(retry_after(
                        response.headers(),
                    )));
                }
                if response.status() == reqwest::StatusCode::NOT_FOUND {
                    return Err(FetchAttemptError::Provider(
                        PackumentProviderError::NotFound,
                    ));
                }
                if !response.status().is_success() {
                    return Err(FetchAttemptError::Provider(
                        PackumentProviderError::HttpStatus {
                            status: response.status().as_u16(),
                        },
                    ));
                }
                if response
                    .content_length()
                    .is_some_and(|length| length > self.max_packument_bytes as u64)
                {
                    return Err(FetchAttemptError::Provider(
                        PackumentProviderError::ResponseTooLarge {
                            limit: self.max_packument_bytes,
                        },
                    ));
                }
                response
                    .bytes()
                    .await
                    .map_err(|_| FetchAttemptError::Provider(PackumentProviderError::ResponseBody))
            })
            .await
            .map_err(|_| PackumentProviderError::Timeout)?;

            match attempt {
                Ok(bytes) => {
                    if bytes.len() > self.max_packument_bytes {
                        return Err(PackumentProviderError::ResponseTooLarge {
                            limit: self.max_packument_bytes,
                        });
                    }
                    return super::packument::parse_packument(bytes.as_ref())
                        .map_err(PackumentProviderError::InvalidPackument);
                }
                Err(FetchAttemptError::Provider(error)) => return Err(error),
                Err(FetchAttemptError::RateLimited(_)) if retry == MAX_RATE_LIMIT_RETRIES => {
                    return Err(PackumentProviderError::HttpStatus { status: 429 });
                }
                Err(FetchAttemptError::RateLimited(retry_after)) => {
                    tokio::time::sleep(rate_limit_delay(retry_after, retry)).await;
                }
            }
        }

        unreachable!("the final rate-limited request returns an error")
    }
}

enum FetchAttemptError {
    Provider(PackumentProviderError),
    RateLimited(Option<Duration>),
}

fn retry_after(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    let value = headers.get(RETRY_AFTER)?.to_str().ok()?;
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    let retry_at = httpdate::parse_http_date(value).ok()?;
    Some(
        retry_at
            .duration_since(SystemTime::now())
            .unwrap_or(Duration::ZERO),
    )
}

fn rate_limit_delay(retry_after: Option<Duration>, retry: usize) -> Duration {
    let maximum_jitter = u64::try_from(RATE_LIMIT_BACKOFFS[retry].as_millis())
        .expect("rate-limit backoff fits in milliseconds");
    let jitter = Duration::from_millis(fastrand::u64(0..=maximum_jitter));
    retry_after.unwrap_or(Duration::ZERO).saturating_add(jitter)
}

fn map_request_error(error: reqwest::Error) -> PackumentProviderError {
    if error.is_timeout() {
        PackumentProviderError::Timeout
    } else if error.is_connect() {
        PackumentProviderError::Connection
    } else {
        PackumentProviderError::Request
    }
}

/// A registry access failure safe to return to callers.
#[derive(Debug, Error)]
pub enum PackumentProviderError {
    #[error("packument was not found")]
    NotFound,
    #[error("registry connection failed")]
    Connection,
    #[error("registry request failed")]
    Request,
    #[error("registry request timed out")]
    Timeout,
    #[error("registry returned HTTP status {status}")]
    HttpStatus { status: u16 },
    #[error("registry response exceeds the {limit}-byte packument limit")]
    ResponseTooLarge { limit: usize },
    #[error("failed to read registry response body")]
    ResponseBody,
    #[error("registry returned an invalid packument")]
    InvalidPackument(#[source] PackumentParseError),
}

/// The declaration that attempted to add an edge beyond the elaboration limit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EdgeLimitOrigin {
    RootDependency {
        dependency: Box<str>,
    },
    Dependency {
        parent: PackageVersion,
        dependency: Box<str>,
    },
}

impl std::fmt::Display for EdgeLimitOrigin {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RootDependency { dependency } => {
                write!(formatter, "root dependency {dependency}")
            }
            Self::Dependency { parent, dependency } => {
                write!(
                    formatter,
                    "dependency {dependency} declared by {}@{}",
                    parent.name, parent.version
                )
            }
        }
    }
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
    #[error(
        "elaboration exceeded dependency-edge limit of {limit}: {edge_count} edges were already recorded while expanding {origin}"
    )]
    EdgeLimitExceeded {
        limit: usize,
        edge_count: usize,
        origin: EdgeLimitOrigin,
    },
    #[error(
        "elaboration exceeded derivation-path limit of {limit}: {derivation_count} paths were already recorded while adding a path of length {path_length} for {package}@{version}"
    )]
    DerivationLimitExceeded {
        limit: usize,
        derivation_count: usize,
        package: Box<str>,
        version: Box<str>,
        path_length: usize,
    },
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
    elaborate_with_progress(source, provider, limits, Arc::new(NoopElaborationProgress)).await
}

/// Expand every NPM package version reachable from `source`, reporting progress.
pub async fn elaborate_with_progress(
    source: &NpmPackageJson,
    provider: Arc<dyn PackumentProvider>,
    limits: ElaborationLimits,
    progress: Arc<dyn ElaborationProgressReporter>,
) -> Result<ElaborationResult, ElaborationError> {
    limits.validate()?;
    progress.report(ElaborationProgress::Started);
    let cache = PackumentCache::with_progress(progress.clone());
    let mut workers = WorkerPool::new(
        limits.worker_concurrency,
        limits.work_queue_capacity,
        cache.clone(),
        provider.clone(),
    );
    let result = tokio::time::timeout(
        limits.total_run_timeout,
        elaborate_inner(
            source,
            provider,
            &limits,
            &cache,
            workers.sender(),
            progress.as_ref(),
        ),
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
    progress: &dyn ElaborationProgressReporter,
) -> Result<ElaborationResult, ElaborationError> {
    let mut state = SupervisorState::default();
    let mut pending = VecDeque::new();
    let mut expansions = VecDeque::from([WorkExpansion::Roots(RootExpansion::new(
        source.root_dependencies(),
    ))]);
    let mut replies = FuturesUnordered::new();
    let max_in_flight = limits
        .worker_concurrency
        .saturating_add(limits.work_queue_capacity);
    let mut completed_work_items = 0;
    let mut last_snapshot = None;
    report_scheduler_progress(
        progress,
        &state,
        pending.len(),
        replies.len(),
        completed_work_items,
        &mut last_snapshot,
    );

    while !pending.is_empty() || !expansions.is_empty() || !replies.is_empty() {
        dispatch_pending(&mut pending, &mut replies, work_sender, max_in_flight)?;

        if let Some(mut expansion) = expansions.pop_front() {
            if pending.len() < limits.max_queued_work {
                if let Some(work) = expansion
                    .next_work(&mut state, cache, provider.as_ref(), limits)
                    .await?
                {
                    if state.record_derivation(
                        work.package.clone(),
                        work.derivation.clone(),
                        limits,
                    )? {
                        pending.push_back(work);
                    }
                    expansions.push_back(expansion);
                }
                report_scheduler_progress(
                    progress,
                    &state,
                    pending.len(),
                    replies.len(),
                    completed_work_items,
                    &mut last_snapshot,
                );
                continue;
            }
            expansions.push_front(expansion);
        }

        let report = replies
            .next()
            .await
            .expect("work is pending only when a worker reply is outstanding")
            .map_err(|_| ElaborationError::WorkerStopped)??;
        completed_work_items = completed_work_items
            .checked_add(1)
            .expect("completed work item count fits in usize");
        state
            .repositories
            .insert(report.package.clone(), report.source_repository.clone());
        expansions.push_back(WorkExpansion::Report(ReportExpansion::new(report)));
        report_scheduler_progress(
            progress,
            &state,
            pending.len(),
            replies.len(),
            completed_work_items,
            &mut last_snapshot,
        );
    }

    let snapshot = scheduler_snapshot(&state, 0, 0, completed_work_items);
    progress.report(ElaborationProgress::Finished { snapshot });
    Ok(state.finish())
}

fn report_scheduler_progress(
    progress: &dyn ElaborationProgressReporter,
    state: &SupervisorState,
    queued_work: usize,
    in_flight_work: usize,
    completed_work_items: usize,
    last_snapshot: &mut Option<ElaborationProgressSnapshot>,
) {
    let snapshot = scheduler_snapshot(state, queued_work, in_flight_work, completed_work_items);
    if last_snapshot.is_some_and(|previous| previous == snapshot) {
        return;
    }
    progress.report(ElaborationProgress::SchedulerUpdated { snapshot });
    *last_snapshot = Some(snapshot);
}

fn scheduler_snapshot(
    state: &SupervisorState,
    queued_work: usize,
    in_flight_work: usize,
    completed_work_items: usize,
) -> ElaborationProgressSnapshot {
    ElaborationProgressSnapshot {
        packages: state.packages.len(),
        edges: state.edges.len(),
        derivations: state.derivation_count,
        queued_work,
        in_flight_work,
        completed_work_items,
    }
}

fn dispatch_pending(
    pending: &mut VecDeque<PendingWork>,
    replies: &mut FuturesUnordered<oneshot::Receiver<Result<WorkerReport, ElaborationError>>>,
    work_sender: &async_channel::Sender<WorkItem>,
    max_in_flight: usize,
) -> Result<(), ElaborationError> {
    while replies.len() < max_in_flight {
        let Some(work) = pending.pop_front() else {
            break;
        };
        let (reply_sender, reply_receiver) = oneshot::channel();
        match work_sender.try_send(WorkItem {
            package: work.package,
            derivations: vec![work.derivation],
            reply_sender,
        }) {
            Ok(()) => replies.push(reply_receiver),
            Err(TrySendError::Full(work)) => {
                pending.push_front(PendingWork {
                    package: work.package,
                    derivation: work
                        .derivations
                        .into_iter()
                        .next()
                        .expect("scheduled work has one derivation"),
                });
                break;
            }
            Err(TrySendError::Closed(_)) => return Err(ElaborationError::WorkerStopped),
        }
    }
    Ok(())
}

struct PendingWork {
    package: PackageVersion,
    derivation: Vec<PackageVersion>,
}

enum WorkExpansion {
    Roots(RootExpansion),
    Report(ReportExpansion),
}

impl WorkExpansion {
    async fn next_work(
        &mut self,
        state: &mut SupervisorState,
        cache: &PackumentCache,
        provider: &dyn PackumentProvider,
        limits: &ElaborationLimits,
    ) -> Result<Option<PendingWork>, ElaborationError> {
        match self {
            Self::Roots(expansion) => expansion.next_work(state, cache, provider, limits).await,
            Self::Report(expansion) => expansion.next_work(state, cache, provider, limits).await,
        }
    }
}

struct RootExpansion {
    roots: VecDeque<RootDependency>,
    current: Option<(RootDependency, std::vec::IntoIter<PackageVersion>)>,
}

impl RootExpansion {
    fn new(roots: Vec<RootDependency>) -> Self {
        Self {
            roots: roots.into(),
            current: None,
        }
    }

    async fn next_work(
        &mut self,
        state: &mut SupervisorState,
        cache: &PackumentCache,
        provider: &dyn PackumentProvider,
        limits: &ElaborationLimits,
    ) -> Result<Option<PendingWork>, ElaborationError> {
        loop {
            if let Some((root, versions)) = self.current.as_mut() {
                if let Some(version) = versions.next() {
                    state.record_edge(
                        ElaborationEdge {
                            parent: None,
                            child: version.clone(),
                            root_dependency_kind: Some(root.kind),
                            declared_dependency: root.name.as_str().into(),
                            declared_specification: format_dependency_specification(
                                &root.specification,
                            ),
                        },
                        limits,
                    )?;
                    return Ok(Some(PendingWork {
                        package: version.clone(),
                        derivation: vec![version],
                    }));
                }
                self.current = None;
            }

            let Some(root) = self.roots.pop_front() else {
                return Ok(None);
            };
            let versions = resolve_specification(
                cache,
                provider,
                &root.name,
                &root.specification,
                None,
                &mut state.warnings,
            )
            .await?;
            self.current = Some((root, versions.into_iter()));
        }
    }
}

struct ReportExpansion {
    parent: PackageVersion,
    derivations: Vec<Vec<PackageVersion>>,
    dependencies: VecDeque<DeclaredDependency>,
    current: Option<ResolvedDependencyExpansion>,
}

impl ReportExpansion {
    fn new(report: WorkerReport) -> Self {
        Self {
            parent: report.package,
            derivations: report.derivations,
            dependencies: report.dependencies.into(),
            current: None,
        }
    }

    async fn next_work(
        &mut self,
        state: &mut SupervisorState,
        cache: &PackumentCache,
        provider: &dyn PackumentProvider,
        limits: &ElaborationLimits,
    ) -> Result<Option<PendingWork>, ElaborationError> {
        loop {
            if let Some(current) = self.current.as_mut() {
                let declared_dependency = current.dependency.name.as_str().to_owned();
                let declared_specification =
                    format_dependency_specification(&current.dependency.specification);
                if let Some((version, derivation_index)) = current.next_candidate(&self.derivations)
                {
                    let derivation = &self.derivations[derivation_index];
                    state.record_edge(
                        ElaborationEdge {
                            parent: Some(self.parent.clone()),
                            child: version.clone(),
                            root_dependency_kind: None,
                            declared_dependency: declared_dependency.into_boxed_str(),
                            declared_specification,
                        },
                        limits,
                    )?;
                    if derivation.contains(&version) {
                        continue;
                    }
                    let mut child_derivation = derivation.clone();
                    child_derivation.push(version.clone());
                    return Ok(Some(PendingWork {
                        package: version,
                        derivation: child_derivation,
                    }));
                }
                self.current = None;
            }

            let Some(dependency) = self.dependencies.pop_front() else {
                return Ok(None);
            };
            let versions = resolve_specification(
                cache,
                provider,
                &dependency.name,
                &dependency.specification,
                Some(&self.parent),
                &mut state.warnings,
            )
            .await?;
            self.current = Some(ResolvedDependencyExpansion::new(dependency, versions));
        }
    }
}

struct ResolvedDependencyExpansion {
    dependency: DeclaredDependency,
    versions: Vec<PackageVersion>,
    derivation_index: usize,
    version_index: usize,
}

impl ResolvedDependencyExpansion {
    fn new(dependency: DeclaredDependency, versions: Vec<PackageVersion>) -> Self {
        Self {
            dependency,
            versions,
            derivation_index: 0,
            version_index: 0,
        }
    }

    fn next_candidate(
        &mut self,
        derivations: &[Vec<PackageVersion>],
    ) -> Option<(PackageVersion, usize)> {
        while self.derivation_index < derivations.len() {
            if let Some(version) = self.versions.get(self.version_index) {
                self.version_index = self
                    .version_index
                    .checked_add(1)
                    .expect("version index is within the resolved version list");
                return Some((version.clone(), self.derivation_index));
            }
            self.derivation_index = self
                .derivation_index
                .checked_add(1)
                .expect("derivation index is within the reported derivation list");
            self.version_index = 0;
        }
        None
    }
}

struct PackumentCache {
    entries: Arc<Mutex<HashMap<PkgName, CachedNpmPackument>>>,
    progress: Arc<dyn ElaborationProgressReporter>,
}

type PkgName = Box<str>;
type CachedNpmPackument = Arc<OnceCell<Arc<NpmPackument>>>;

impl Clone for PackumentCache {
    fn clone(&self) -> Self {
        Self {
            entries: self.entries.clone(),
            progress: self.progress.clone(),
        }
    }
}

impl PackumentCache {
    fn with_progress(progress: Arc<dyn ElaborationProgressReporter>) -> Self {
        Self {
            entries: Arc::new(Mutex::new(HashMap::new())),
            progress,
        }
    }

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
            self.progress
                .report(ElaborationProgress::PackumentFetchStarted {
                    package: package_name.clone().into_boxed_str(),
                });
            let fetched = provider.fetch(package).await;
            self.progress
                .report(ElaborationProgress::PackumentFetchCompleted {
                    package: package_name.clone().into_boxed_str(),
                });
            fetched
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
            specification: specification.valid()?.clone(),
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
                (dependency_name.valid() == Some(name))
                    .then(|| specification.valid())
                    .flatten()
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
    derivation_count: usize,
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
            return Err(ElaborationError::EdgeLimitExceeded {
                limit: limits.max_edges,
                edge_count: self.edges.len(),
                origin: EdgeLimitOrigin::from(&edge),
            });
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
        if self.derivation_count == limits.max_derivations {
            return Err(ElaborationError::DerivationLimitExceeded {
                limit: limits.max_derivations,
                derivation_count: self.derivation_count,
                package: package.name.clone(),
                version: package.version,
                path_length: derivation.len(),
            });
        }
        self.packages.entry(package).or_default().insert(derivation);
        self.derivation_count = self
            .derivation_count
            .checked_add(1)
            .expect("derivation count fits in usize");
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

impl From<&ElaborationEdge> for EdgeLimitOrigin {
    fn from(edge: &ElaborationEdge) -> Self {
        match &edge.parent {
            Some(parent) => Self::Dependency {
                parent: parent.clone(),
                dependency: edge.declared_dependency.clone(),
            },
            None => Self::RootDependency {
                dependency: edge.declared_dependency.clone(),
            },
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
        sync::{
            Mutex as StdMutex,
            atomic::{AtomicUsize, Ordering},
        },
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
    fn reports_elaboration_progress_without_changing_the_result() {
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
        let events = Arc::new(StdMutex::new(Vec::new()));
        let reporter_events = events.clone();
        let reporter: Arc<dyn ElaborationProgressReporter> = Arc::new(move |progress| {
            reporter_events
                .lock()
                .expect("progress event lock is not poisoned")
                .push(progress);
        });

        let result = tokio::runtime::Runtime::new()
            .expect("runtime")
            .block_on(elaborate_with_progress(
                &source,
                Arc::new(registry),
                ElaborationLimits::default(),
                reporter,
            ))
            .expect("elaboration succeeds");
        let events = events.lock().expect("progress event lock is not poisoned");

        assert_eq!(result.packages.len(), 2);
        assert_eq!(result.edges.len(), 2);
        assert!(matches!(events.first(), Some(ElaborationProgress::Started)));
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, ElaborationProgress::PackumentFetchStarted { .. }))
                .count(),
            2
        );
        assert!(events.iter().any(|event| {
            matches!(
                event,
                ElaborationProgress::Finished { snapshot }
                    if snapshot.packages == 2
                        && snapshot.edges == 2
                        && snapshot.derivations == 2
                        && snapshot.queued_work == 0
                        && snapshot.in_flight_work == 0
            )
        }));
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

    #[test]
    fn ignores_invalid_packument_dependency_specifications() {
        let source = source(json!({ "dependencies": { "root": "1.0.0" } }));
        let registry = MockRegistry {
            packuments: BTreeMap::from([(
                "root".to_owned(),
                packument("root", "1.0.0", json!({ "async_testing": "" })),
            )]),
        };

        let result =
            run(&source, registry).expect("invalid upstream specification is not resolved");

        assert_eq!(result.packages.len(), 1);
        assert!(
            result
                .packages
                .iter()
                .all(|package| package.package.name.as_ref() != "async_testing")
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

        assert_eq!(
            error.to_string(),
            "elaboration exceeded dependency-edge limit of 1: 1 edges were already recorded while expanding dependency child declared by root@1.0.0"
        );
        assert!(matches!(
            error,
            ElaborationError::EdgeLimitExceeded {
                limit: 1,
                edge_count: 1,
                origin: EdgeLimitOrigin::Dependency { parent, dependency },
            } if parent.name.as_ref() == "root"
                && parent.version.as_ref() == "1.0.0"
                && dependency.as_ref() == "child"
        ));
    }

    #[test]
    fn fails_when_the_derivation_limit_is_exceeded() {
        let source = source(json!({ "dependencies": { "root": "1.0.0" } }));
        let registry = MockRegistry {
            packuments: BTreeMap::from([
                (
                    "root".to_owned(),
                    packument(
                        "root",
                        "1.0.0",
                        json!({ "left": "1.0.0", "right": "1.0.0" }),
                    ),
                ),
                (
                    "left".to_owned(),
                    packument("left", "1.0.0", json!({ "shared": "1.0.0" })),
                ),
                (
                    "right".to_owned(),
                    packument("right", "1.0.0", json!({ "shared": "1.0.0" })),
                ),
                ("shared".to_owned(), packument("shared", "1.0.0", json!({}))),
            ]),
        };
        let limits = ElaborationLimits {
            max_derivations: 3,
            ..ElaborationLimits::default()
        };

        let error = tokio::runtime::Runtime::new()
            .expect("runtime")
            .block_on(elaborate(&source, Arc::new(registry), limits))
            .expect_err("derivation limit is enforced");

        assert_eq!(
            error.to_string(),
            "elaboration exceeded derivation-path limit of 3: 3 paths were already recorded while adding a path of length 3 for shared@1.0.0"
        );
        assert!(matches!(
            error,
            ElaborationError::DerivationLimitExceeded {
                limit: 3,
                derivation_count: 3,
                package,
                version,
                path_length: 3,
            } if package.as_ref() == "shared" && version.as_ref() == "1.0.0"
        ));
    }

    #[test]
    fn backpressures_when_the_pending_work_limit_is_reached() {
        let source = source(json!({ "dependencies": { "root": "1.0.0" } }));
        let registry = MockRegistry {
            packuments: BTreeMap::from([
                (
                    "root".to_owned(),
                    packument(
                        "root",
                        "1.0.0",
                        json!({ "left": "1.0.0", "right": "1.0.0" }),
                    ),
                ),
                ("left".to_owned(), packument("left", "1.0.0", json!({}))),
                ("right".to_owned(), packument("right", "1.0.0", json!({}))),
            ]),
        };
        let limits = ElaborationLimits {
            worker_concurrency: 1,
            work_queue_capacity: 1,
            max_queued_work: 1,
            ..ElaborationLimits::default()
        };

        let result = tokio::runtime::Runtime::new()
            .expect("runtime")
            .block_on(elaborate(&source, Arc::new(registry), limits))
            .expect("queued work is backpressured");

        assert_eq!(result.packages.len(), 3);
        assert_eq!(result.edges.len(), 3);
    }

    #[test]
    fn resumes_multi_level_expansion_after_backpressure() {
        let source = source(json!({ "dependencies": { "root": "1.0.0" } }));
        let registry = MockRegistry {
            packuments: BTreeMap::from([
                (
                    "root".to_owned(),
                    packument(
                        "root",
                        "1.0.0",
                        json!({ "left": "1.0.0", "right": "1.0.0" }),
                    ),
                ),
                (
                    "left".to_owned(),
                    packument("left", "1.0.0", json!({ "left-leaf": "1.0.0" })),
                ),
                (
                    "right".to_owned(),
                    packument("right", "1.0.0", json!({ "right-leaf": "1.0.0" })),
                ),
                (
                    "left-leaf".to_owned(),
                    packument("left-leaf", "1.0.0", json!({})),
                ),
                (
                    "right-leaf".to_owned(),
                    packument("right-leaf", "1.0.0", json!({})),
                ),
            ]),
        };
        let limits = ElaborationLimits {
            worker_concurrency: 1,
            work_queue_capacity: 1,
            max_queued_work: 1,
            ..ElaborationLimits::default()
        };

        let result = tokio::runtime::Runtime::new()
            .expect("runtime")
            .block_on(elaborate(&source, Arc::new(registry), limits))
            .expect("queued work is backpressured across levels");

        assert_eq!(result.packages.len(), 5);
        assert_eq!(result.edges.len(), 5);
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

    #[tokio::test]
    async fn registry_client_reports_unsuccessful_statuses() {
        let server = MockServer::start();
        let _mock = server.mock(|when, then| {
            when.method(GET).path("/root");
            then.status(503);
        });
        let client = NpmRegistryClient::new(
            Url::parse(&format!("{}/", server.base_url())).expect("valid mock URL"),
            1024,
            Duration::from_secs(1),
        )
        .expect("valid client");
        let package = NpmPackageName::parse("root".to_owned()).expect("valid package name");

        let error = client
            .fetch(&package)
            .await
            .expect_err("unsuccessful status is reported");

        assert_eq!(error.to_string(), "registry returned HTTP status 503");
        assert!(matches!(
            error,
            PackumentProviderError::HttpStatus { status: 503 }
        ));
    }

    #[test]
    fn parses_retry_after_values() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(RETRY_AFTER, reqwest::header::HeaderValue::from_static("42"));
        assert_eq!(retry_after(&headers), Some(Duration::from_secs(42)));

        headers.insert(
            RETRY_AFTER,
            reqwest::header::HeaderValue::from_static("Sun, 06 Nov 1994 08:49:37 GMT"),
        );
        assert_eq!(retry_after(&headers), Some(Duration::ZERO));

        headers.insert(
            RETRY_AFTER,
            reqwest::header::HeaderValue::from_static("not a retry delay"),
        );
        assert_eq!(retry_after(&headers), None);
    }

    #[test]
    fn rate_limit_delay_honors_retry_after_and_bounds_jitter() {
        let retry_after = Duration::from_secs(2);
        let delay = rate_limit_delay(Some(retry_after), 0);

        assert!(delay >= retry_after);
        assert!(delay <= retry_after.saturating_add(RATE_LIMIT_BACKOFFS[0]));
    }

    #[tokio::test]
    async fn registry_client_retries_rate_limits() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/root");
            then.status(429).header("Retry-After", "0");
        });
        let client = NpmRegistryClient::new(
            Url::parse(&format!("{}/", server.base_url())).expect("valid mock URL"),
            1024,
            Duration::from_secs(1),
        )
        .expect("valid client");
        let package = NpmPackageName::parse("root".to_owned()).expect("valid package name");

        let error = client
            .fetch(&package)
            .await
            .expect_err("persistent rate limit is reported");

        assert!(matches!(
            error,
            PackumentProviderError::HttpStatus { status: 429 }
        ));
        mock.assert_calls(MAX_RATE_LIMIT_RETRIES.saturating_add(1));
    }

    #[tokio::test]
    async fn registry_client_recovers_after_a_rate_limit() {
        let server = MockServer::start();
        let rate_limited = server.mock(|when, then| {
            when.method(GET).path("/root");
            then.status(429).header("Retry-After", "0");
        });
        let client = NpmRegistryClient::new(
            Url::parse(&format!("{}/", server.base_url())).expect("valid mock URL"),
            1024,
            Duration::from_secs(1),
        )
        .expect("valid client");
        let package = NpmPackageName::parse("root".to_owned()).expect("valid package name");
        let fetch = tokio::spawn(async move { client.fetch(&package).await });

        tokio::time::timeout(Duration::from_secs(1), async {
            while rate_limited.calls_async().await == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("rate-limited request arrives");
        rate_limited.assert_calls(1);
        rate_limited.delete_async().await;
        let success = server.mock(|when, then| {
            when.method(GET).path("/root");
            then.status(200).body(packument("root", "1.0.0", json!({})));
        });

        let packument = fetch
            .await
            .expect("fetch task completes")
            .expect("retry succeeds after the rate limit");

        assert_eq!(packument.name.as_str(), "root");
        success.assert_calls(1);
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
