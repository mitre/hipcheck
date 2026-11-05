# RFD 0005: Package Source Elaboration

## Status

Proposed

## Date

2026-09-01

## Table of Contents

[[_TOC_]]

## Summary

Night Vision should elaborate an NPM `package.json` package source into the
set of concrete NPM package versions reachable from it. The result must retain
the dependency graph necessary to explain every acyclic derivation path from a
root dependency to each reachable version.

Elaboration is a bounded, concurrent operation. A Tokio supervisor owns one
authoritative in-memory result, dispatches concrete package-version work to a
configurable set of worker tasks, and atomically publishes a completed result.
Workers obtain validated NPM packuments through a per-run cache, so concurrent
work never refetches a package's metadata unnecessarily.

The first implementation supports only NPM `package.json` sources and a
single configured NPM registry. It powers both the `nvdb package-source
resolve` command and asynchronous resolution for API-submitted sources.

## Background

Package manifests describe dependency specifications, not a single installed
dependency tree. For example, an NPM range can match many published versions.
Night Vision deliberately expands all matching published versions rather than
relying on a lockfile, so it can identify the complete set of versions that a
source may reach. The existing [package-resolution guide][resolution-guide]
establishes that this is an intentionally over-inclusive analysis and that
derivation paths distinguish both package name and version.

`nv-common` already validates NPM package manifests, parses packuments, and
evaluates NPM version ranges. The repository does not yet have a resolver
orchestrator, durable derivation-edge model, package-source lifecycle state, or
implementation for the existing package-source CLI commands.

## Goals

- Expand supported NPM dependency specifications into concrete package
  versions from the configured registry.
- Preserve enough normalized information to derive all acyclic paths to every
  reachable version.
- Bound registry work, in-memory state, and path expansion.
- Make successful publication atomic and leave a prior successful result
  visible if re-elaboration fails.
- Expose progress, terminal failures, and nonfatal unsupported-specification
  warnings through the CLI and API.

## Non-Goals

- Reproduce npm's lockfile unification or installation layout algorithm.
- Support local files, workspaces, Git repositories, direct URLs, or an
  alternate package ecosystem.
- Persist registry metadata beyond the lifetime of one elaboration run.
- Guarantee that a reachable package is safe or installed in every possible
  lockfile derived from the source.

## Decision

### Resolve work under one supervisor

Each elaboration run has one Tokio supervisor task. It is the sole owner of
the mutable result and the only producer for a bounded `async-channel` work
queue. It spawns a configured number of workers; all workers consume from the
same queue. Each work item includes a bounded one-shot return channel, through
which its worker sends exactly one report to the supervisor.

A work item contains a concrete package/version and the derivation prefixes
that have not yet been propagated through that version. A worker retrieves the
package's packument, locates the concrete version, and reports the package and
its declared dependencies to the supervisor. Workers neither modify the result
nor enqueue additional work themselves.

The supervisor deduplicates version nodes and dependency edges, appends newly
learned derivation prefixes, and sends follow-up work. If a package/version
receives a prefix after it has already been processed, the supervisor requeues
only those unpropagated prefixes. This is necessary to propagate every path
through shared subgraphs while avoiding duplicate path propagation. The
supervisor tracks outstanding work; once it reaches zero, it closes the work
sender, joins workers, validates the completed result, and starts publication.

All traversal state is source-local. A cycle is recorded as an edge but is not
expanded through a prefix that already contains the target package/version.
Paths presented to callers therefore are finite, acyclic paths; cycle edges
can be represented as terminal markers in explanatory output.

### Resolve NPM specifications conservatively

The resolver uses the existing NPM range evaluator to select every published
version satisfying a registry range. A dist-tag resolves to the version named
by the packument's `dist-tags` map. An `npm:` alias resolves its target package
and target specification while retaining the declared alias on its edge.

The resolver traverses every dependency collection available in a supported
manifest or package version: dependencies, dev dependencies, peer
dependencies, optional dependencies, and bundle dependencies. A bundled
dependency uses its declaration from the corresponding dependency map.

File, link, workspace, Git, repository shorthand, and direct-URL
specifications are not registry-resolvable in v1. They are skipped and saved
as structured warnings with the declaring package/version, dependency name,
specification kind, and safe explanatory message. Invalid ranges, an unknown
tag, a missing concrete version, registry transport or HTTP failure, and an
invalid packument are fatal run failures.

### Cache packuments for one run

Each run owns a cache keyed by NPM package name, not package version. The cache
stores validated packuments because a packument supplies metadata for all of a
package's versions. Concurrent requests for the same package coalesce onto one
in-flight fetch and receive the same result. The cache is discarded when the
run ends; it is neither process-global nor persisted.

The configured registry defaults to `https://registry.npmjs.org/`. Deployments
and controlled tests may configure one validated registry base URL, but a
source cannot select a registry.

### Persist a source-scoped graph atomically

The database model must distinguish a globally identified package from a
version reachable from one source, and must add source-scoped parent-to-child
edges. Root edges retain the root dependency kind and declared specification.
Package-version and edge rows are normalized; complete derivation paths are
generated from the stored graph for CLI and API output rather than materialized
as path rows.

Package sources gain persisted processing state, bounded failure diagnostics,
and resolution warnings. A successful re-resolution constructs a new result in
memory and replaces the source's prior reachable-version, edge, and warning
snapshot in one short database transaction. A failed run changes only its
lifecycle diagnostic state and leaves the last completed graph available.

## Interfaces and Lifecycle

`nvdb package-source resolve <SOURCE-ID>` runs the shared resolver
synchronously and reports a deterministic summary. `package-source versions`
lists reachable PURLs and derivation information in deterministic order.

The API validates and stores an NPM `package.json`, returns `202 Accepted`,
and launches a source-local background elaboration task. `GET
/package-sources/{id}` exposes `processing`, `completed`,
`completed-with-warnings`, and `failed` states. Completed representations
return reachable versions and derivation information; warning and failure
details are bounded and safe for API output. Invalid source input is rejected
before a background task is created.

The resolver has a typed configuration for registry URL, worker concurrency,
request and total-run timeouts, channel capacity, maximum packument size,
maximum nodes, maximum edges, maximum queued work, and maximum derivation
paths. All limits are finite and validated at startup. Worker concurrency is a
positive override or a computed default of
`min(max(4, 4 * Tokio worker threads), 32)`. Initial numeric budget defaults
will be calibrated during implementation and documented with their resource
rationale.

When any configured limit, worker, registry, parsing, cancellation, or
publication failure occurs, the supervisor stops dispatching work, closes and
joins workers, records a bounded failure, and does not publish a partial
snapshot.

## Implementation Plan

1. Add an `nv-common` elaboration service with typed work, report, graph,
   warning, error, cache, and configuration interfaces. Add `async-channel`
   and an asynchronous NPM registry client.
2. Implement the supervisor/worker lifecycle, coalescing per-run packument
   cache, range/tag/alias selection, all supported dependency collections,
   cycle handling, and budget enforcement.
3. Add migrations and SeaORM entities for source lifecycle metadata,
   reachable versions, root metadata, derivation edges, and warnings. Implement
   atomic snapshot replacement and failed-run recording.
4. Wire the shared service into `nvdb package-source resolve` and the server's
   package-source submission/status lifecycle. Update API schemas, OpenAPI
   output, command reference, and package-resolution documentation.
5. Add unit and mocked-registry integration coverage for range, tag, alias,
   warning, cache, concurrency, multiple-path, cycle, failure, limit, atomic
   replacement, CLI, and API lifecycle behavior.

## Open Questions

None for the initial implementation. Future work may add source-specific
registries, lockfile-aware resolution, non-registry source types, workspace
bundles, persistent registry caching, and additional package ecosystems.

[resolution-guide]: ../backend/resolving-packages.md
