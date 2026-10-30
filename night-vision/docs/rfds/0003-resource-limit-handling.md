# RFD 0003: Resource Limit Handling for External Ingest

## Status

Proposed

## Date

2026-07-14

## Table of Contents

[[_TOC_]]

## Summary

Night Vision should treat every externally sourced ingest as a resource-budgeted
operation. Per-item limits are necessary but insufficient: the CVE List Git
worker must also bound transfer size, object and path counts, aggregate parsed
and staged data, database work, cache disk use, and repeated failures.

The system should reject work that exceeds a configured budget before it can
exhaust shared capacity. Limit failures must leave the previously published CVE
snapshot available, clean up partial state, record an operator-actionable sync
failure, and retry with bounded exponential backoff rather than immediately
repeating expensive work.

This RFD establishes the resource-limit model and an implementation sequence.
It applies first to CVE List ingest and should be reused by future remote Git,
registry, archive, and analysis integrations.

## Background

The current CVE pipeline limits individual record blobs, parser concurrency,
the parsed-record write channel, and total sync duration. Those controls cap
some local work but do not bound the amount of external Git data accepted or
the aggregate state retained by an import.

In particular, clone and fetch can write arbitrary pack data to the checkout
cache; Git path listing currently materializes complete command output and path
lists; a complete snapshot stages every JSON record before publishing it; and
the worker holds a transaction-scoped advisory lock through remote I/O and
import work. A persistent upstream failure can repeat the expensive first-sync
budget indefinitely.

The `nv-server` threat model already identifies untrusted upstream content,
the writable checkout cache, database capacity, and worker capacity as
availability-sensitive boundaries. This RFD turns that direction into a
concrete, reusable design.

## Goals

- Bound all resource dimensions controlled by external ingest data.
- Preserve the last known-good published snapshot when a new import fails.
- Avoid holding database transactions or advisory locks across remote I/O and
  long-lived parsing work.
- Make limit decisions configurable, visible, measurable, and testable.
- Slow recurring failures without hiding their freshness impact from operators.
- Provide a common limit and failure model for future external integrations.

## Non-Goals

- Guaranteeing that an upstream repository is correct or available.
- Replacing deployment-level CPU, memory, filesystem, or PostgreSQL quotas.
- Making every limit dynamically self-tuning in the first implementation.
- Retaining an unbounded history of rejected input for diagnostics.
- Changing CVE record semantics or the published API merely to add limits.

## Decision

### Budget every stage

Each ingest attempt must use a named budget with a clear owner and enforcement
point. Configuration may set lower limits than deployment defaults, but it must
not permit an unbounded value. Startup validation should reject incompatible
budgets, such as a staged-data limit smaller than one permitted record.

| Stage | Required budgets | Enforcement |
| --- | --- | --- |
| Git transfer | clone/fetch bytes, elapsed time, checkout disk bytes | Before and during transfer; stop Git and remove incomplete transfer state when exceeded. |
| Repository inventory | object count, path count, total path bytes, path length | Stream and count Git output; stop before allocating an unbounded path collection. |
| Record processing | record bytes, parsed-record concurrency, aggregate input bytes | Enforce before allocation and increment aggregate counters as records enter the pipeline. |
| Staging | record count, aggregate JSON bytes, generation disk/database bytes | Reserve or account for each staged record; fail before exceeding the generation budget. |
| Publish | database statement timeout, transaction duration, rows and bytes per publish unit | Keep each transaction small and bounded; publish only a complete generation. |
| Cache lifecycle | checkout quota, retained-object policy, free-space watermark | Check before work, measure after work, and prune or recreate only through an explicit recovery path. |
| Retries | consecutive failures, retry delay, retry window | Classify failures and apply capped exponential backoff with jitter and a circuit-breaker state. |

Budgets should be represented as a single typed ingest-limit configuration
instead of independent ad hoc constants. The effective configuration report
must show every resolved limit and whether it was explicit or computed.

### Stream inventory and processing

Git command wrappers must not use whole-output collection for externally sized
commands. Inventory output should be parsed incrementally, validate each path,
and stop when its count or byte budget is reached. The `cat-file` request list
must be batched so that its input buffer is bounded as well.

The pipeline may retain bounded batches, but it must not retain all discovered
paths or all parsed records merely to preserve order. A stable per-record key
or a bounded sequence number is sufficient where ordering is needed.

### Make staging an explicitly bounded generation

Staging remains the atomic-publication boundary: readers see either the prior
snapshot or a fully validated new generation. A generation must carry its
record count and JSON-byte totals, and staging must refuse additional rows once
its limits would be exceeded.

The service should avoid maintaining a second full JSONB copy longer than
necessary. The implementation may use a staging table, generation-marked live
rows, or another equivalent publish design, provided that it preserves atomic
reader visibility, supports cleanup after cancellation, and fits the configured
database and disk budget.

### Separate coordination from database transactions

The worker needs exclusive coordination, but it does not need an open database
transaction while cloning, fetching, listing, or parsing. It should acquire a
short-lived lease or advisory lock to establish ownership, record the running
generation, and release database transactions between bounded operations.

The final publish should use a short transaction with PostgreSQL statement and
transaction timeout settings. If a lease is used, it must be renewed safely and
must prevent two live publishers from committing the same source generation.
Cancellation, timeout, and process death must leave incomplete staging
unpublished and eligible for cleanup.

### Define limit failures and retries

Limit exceedance is an expected operational failure, distinct from malformed
records and transient transport/database failures. Sync metadata and logs must
record the limit name, observed value, configured maximum, source, generation,
and resolved commit when known. Do not store raw oversized content or unbounded
Git stderr in that metadata.

The worker should use capped exponential backoff with jitter for transient
failures. A source that has never completed successfully must not repeatedly
receive the largest first-sync timeout at the normal interval. After a bounded
number of consecutive failures or limit failures, the worker should enter a
visible cooldown state and make only scheduled probe attempts. A successful
sync resets the failure counter. Operators must be able to inspect the current
backoff, the last failure class, and the next attempt time.

## Limit Selection and Operations

Defaults must be selected from an explicit host and PostgreSQL capacity budget,
not from the observed size of the public CVE repository alone. Deployment
documentation should state the assumed cache-volume size, free-space
watermark, database storage/WAL headroom, connection-pool allocation, and the
maximum time that background work may consume shared capacity.

The limits should have conservative defaults and documented relationships:

- Aggregate staged JSON must fit within the database budget after accounting
  for JSONB, indexes, WAL, and the currently published snapshot.
- Cache quota must leave enough free space for a safe transfer and recovery.
- Transfer and inventory budgets must be no greater than cache and staging
  budgets can support.
- Channel capacity times maximum record size must fit the worker memory budget.
- Publish batch limits must fit the PostgreSQL statement timeout and parameter
  limits.

Metrics should report attempt outcome, failure class, each observed high-water
mark, cache use, staging use, transaction duration, retry count, and snapshot
freshness. Alerts should distinguish an upstream outage from a rejected source
that exceeds a local safety budget.

## Implementation Plan

### Phase 1: Establish the limit contract

1. Add a typed `CveListIngestLimits` configuration with finite maxima for every
   budget in this RFD.
2. Validate cross-field relationships at startup and include effective values
   in `nvdb cve config`, server startup logs, and operator documentation.
3. Add a structured `ResourceLimitExceeded` error carrying safe limit and
   observed-value fields; persist a bounded rendering in sync-run metadata.
4. Add metrics and health fields for limits, resource high-water marks, retry
   state, and next scheduled attempt.

### Phase 2: Bound Git and inventory work

1. Replace whole-output Git inventory commands with streaming parsers.
2. Enforce path/object/count and total-path-byte limits while listing changed
   and full-snapshot files.
3. Batch `cat-file` requests under a byte and path-count budget.
4. Add transfer accounting, checkout free-space checks, a cache quota, and
   documented recovery for incomplete clone/fetch state.
5. Bound captured subprocess stderr and ensure cancellation kills child
   processes and closes their pipes.

### Phase 3: Bound storage and publication

1. Account for accepted record count and canonical JSON bytes before staging
   each batch.
2. Enforce generation limits in application code and, where practical, with
   database constraints or a generation-accounting table.
3. Refactor exclusive-sync coordination so that no transaction remains open
   across remote I/O or parsing.
4. Apply database statement and transaction timeouts to the short staging and
   publish units.
5. Verify cancellation and cleanup cannot publish partial data or leak staged
   rows beyond retention limits.

### Phase 4: Make repeated failure safe

1. Define failure classes: resource limit, malformed source, transient Git,
   transient database, and internal failure.
2. Add capped exponential backoff, jitter, consecutive-failure tracking, and a
   visible cooldown/probe policy.
3. Use a separate, bounded bootstrap policy for the first sync; do not reset
   its expensive allowance after every failure.
4. Add operator guidance for increasing a budget, clearing a corrupt cache,
   and accepting or rejecting an unexpectedly large upstream revision.

## Verification Plan

Add deterministic tests for each limit boundary and for cleanup behavior:

- clone/fetch and checkout-cache quota exhaustion;
- oversized or unterminated Git output, excessive path counts, and excessive
  total path bytes;
- `cat-file` batching, object-count limits, and aggregate record-byte limits;
- staged JSON and record-count budget exhaustion with the old snapshot still
  readable;
- statement timeout, transaction timeout, cancellation, and stale-generation
  cleanup;
- partial clone/fetch recovery without trusting a damaged checkout;
- retry progression, jitter bounds, cooldown entry, probe behavior, and reset
  after success; and
- metrics, sync metadata, and health output that expose safe failure details
  without external content or secrets.

Run the backend CI suite and PostgreSQL integration coverage for each phase.
Before enabling a limit in a deployment, exercise it in a capacity test using
representative data plus deliberately oversized inventories and snapshots.

## Consequences

This design adds configuration, accounting, metrics, and failure states to a
previously simple worker. It also makes an oversized upstream update fail
closed until an operator explicitly changes the budget or source policy.

In return, an external repository cannot silently consume arbitrary memory,
disk, database capacity, or retry time. The service retains a usable prior CVE
snapshot during a rejected or failed update and gives operators evidence to
make a capacity decision.

## Open Questions

1. Which deployment profiles should ship: development, single-node production,
   and larger shared PostgreSQL deployments?
2. Should the database own generation-byte accounting through triggers, or is
   application accounting plus database quotas sufficient for the first phase?
3. What freshness policy should dependent API behavior use while a source is in
   cooldown: serve stale data with a warning, or fail closed after an SLO?
4. Does the deployment platform provide reliable per-volume quota telemetry, or
   must the worker enforce its own filesystem measurements?
