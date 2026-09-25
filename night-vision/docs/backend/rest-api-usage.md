# REST API Usage

This guide describes the REST API currently implemented by `nv-server`.
Use it for local development and frontend integration against the present API
surface. The generated OpenAPI description remains the source of truth for
schemas, paths, and response codes.

[[_TOC_]]

## Local Server

The local sample configuration in `backend/nv-server.spookey` binds
`nv-server` to `127.0.0.1:8080`.

```sh
curl http://127.0.0.1:8080/health
```

The package-source and assessment endpoints in this local guide have no user
or tenant identity model. Treat access to them as single-operator authority;
do not assume production authentication, authorization, or deployment behavior.
Operator diagnostics and optional raw Hipcheck evidence require a configured
diagnostics bearer token.

For local startup, health checks, logs, and recovery steps, see the
[backend operations runbook](./operations-runbook.md).

## JSON Conventions

Request and response bodies use JSON. Field names are camelCase, matching the
OpenAPI schema generated from `backend/nv-server-api/src/lib.rs`.

Current examples include:

- `displayName`
- `fileName`
- `createdAt`
- `versionedPackages`

Package-source status responses are tagged by a `status` field. The current
schemas include `pending`, `processing`, `completed`, `completed-with-warnings`,
`failed`, and `cancelled`. Their durable semantics are defined in the
[package-source lifecycle](./package-source-lifecycle.md). The guide below
describes only endpoints implemented today.

Upgrade-assessment status responses use `pending`, `completed`, and `failed`.
The exposure read models use `not-assessed`, `processing`, `completed`, `failed`,
and `unavailable` to describe what a browser can currently show.

`GET /assessments/{id}` reports the assessment run state in a `state` field
(for example, `queued`). Package-source and upgrade-assessment responses use a
`status` field.

## Endpoints

### `GET /health`

Check whether the server process can answer HTTP requests. This is the public
liveness endpoint: it is safe to expose to untrusted callers and deliberately
does not query the database or report operational state.

Request:

```sh
curl http://127.0.0.1:8080/health
```

Successful response:

```http
HTTP/1.1 200 OK
Content-Type: application/json
```

```json
{
  "status": "ok"
}
```

`200 OK` means the server process answered the request. It does not indicate
database availability, CVE List freshness, or whether the initial sync has
completed.

### `GET /health/diagnostics`

Read CVE List and KEV ingest state for operators. The endpoint is disabled unless
`health-diagnostics-token-file` is configured. When enabled, callers must send
the configured bearer token. Missing, malformed, duplicated, or incorrect
credentials receive the same `401 Unauthorized` response and a
`WWW-Authenticate: Bearer` challenge.

```sh
curl http://127.0.0.1:8080/health/diagnostics \
  -H "Authorization: Bearer $(<../.secrets/health-diagnostics-token)"
```

```json
{
  "status": "ok",
  "cveIngest": {
    "recordsAvailable": false,
    "freshness": "unknown",
    "lastSuccessfulSyncAt": null,
    "latestSuccessfulCommit": null,
    "latestRun": null
  },
  "kevIngest": {
    "recordsAvailable": false,
    "freshness": "unknown",
    "lastSuccessfulSyncAt": null,
    "latestRun": null
  }
}
```

`200 OK` means the handler read its diagnostic state; it does not mean the
initial synchronization has completed. Each dataset is independent.
`recordsAvailable` is `true` when its active records are available; `freshness`
is `current`, `stale`, or `unknown`; and `lastSuccessfulSyncAt` is the most
recent successful or not-modified completion. `latestRun` contains operator
diagnostic detail for the most recent attempt. When diagnostics are disabled,
the endpoint returns `404 Not Found` without querying the database.

### `GET /data-status`

Read the bounded data-freshness summary intended for browser users. This public
endpoint is not a liveness check and does not expose operator diagnostics, raw
errors, source artifacts, commit identifiers, or run counters.

```sh
curl http://127.0.0.1:8080/data-status
```

Each `cveList` and `cisaKev` object independently reports availability,
freshness, the last successful synchronization time, and the latest attempt.
Freshness is `current` when an available snapshot was confirmed within its
configured threshold, `stale` when that snapshot is older, and `unknown` when
no successful snapshot exists. A failed latest attempt is represented by its
attempt status and a fixed safe message; it never refreshes the successful
timestamp. Initial synchronization is identifiable when data is unavailable
and the latest attempt is absent, running, or failed. Stale KEV data means new
KEV records may not yet appear in assessments; it does not make existing
assessment evidence incorrect or establish that a package is safe.

The `assessmentImpact` text is guidance for the data-status trust page planned
by issue #96. It must be rendered as text, not markup. This endpoint provides
no manual synchronization control.

`GET /health` remains the only public liveness endpoint. It never fails merely
because either vulnerability dataset is stale or unavailable.

### `POST /package-sources`

Submit a package source for processing. The endpoint accepts only
`application/json` requests with exactly the `displayName`, `fileName`, and
`contents` fields. `displayName` is required, trimmed before storage, and
limited to 120 Unicode characters. It is the user-facing identity used in
package-source lists. Legacy sources receive a stable
`package.json — <source ID prefix>` label during migration.
`fileName` must be exactly `package.json`; paths, alternative file names, and
additional fields are rejected. `contents` must be a valid npm `package.json`
document and must not exceed `package-source-contents-max-bytes` (1 MiB by
default).

Request:

```sh
curl -X POST http://127.0.0.1:8080/package-sources \
  -H 'Content-Type: application/json' \
  -d '{
    "displayName": "Example application",
    "fileName": "package.json",
    "contents": "{\"dependencies\":{\"react\":\"18.2.0\"}}"
  }'
```

Successful response:

```http
HTTP/1.1 202 Accepted
Content-Type: application/json
```

```json
{
  "id": "00000000-0000-0000-0000-000000000001"
}
```

The `202 Accepted` status means the server persisted the source and accepted
it for processing; it does not promise that processing has started, and
submission is never rejected for lack of active-resolution capacity. A
background dispatcher claims durably `pending` sources and retries eligible
failures automatically. Use the returned `id` with `GET /package-sources/{id}`
to check the package-source status. Cancellation, deletion, retry, and
operator-triggered retention are described in the
[package-source lifecycle](./package-source-lifecycle.md).

### `GET /package-sources`

Read a bounded package-source work list for the browser. Index rows never
include manifests, dependency graphs, warning details, or diagnostics.

`limit` is 1 through 100 (default 25); `cursor` is the decimal `nextCursor`
returned by the prior page. The server rejects malformed cursors and bounds
traversal to 10,000 summary candidates. `filter` is `all` (default),
`needs-attention`, `processing`, or `failed`; processing includes pending and
processing states. Needs attention includes failures, completed sources with
warnings, and completed sources whose CVE or KEV data is unavailable.

`query` is an optional case-insensitive source-identity substring of at most
100 characters. Supported `sort` keys are `activity` (default), `identity`,
`lifecycle`, `resolution-time`, `reachable-packages`, and `exposures`.
`direction` is `desc` (default) or `asc`; source ID is the stable tie-breaker.

```json
{
  "items": [{
    "id": "00000000-0000-0000-0000-000000000001",
    "displayName": "Example application",
    "ecosystem": "npm",
    "lifecycle": "completed",
    "createdAt": "2000-01-01T00:00:00Z",
    "activityAt": "2000-01-01T00:01:00Z",
    "resolutionAt": "2000-01-01T00:01:00Z",
    "reachablePackageCount": 42,
    "exposureCount": 0,
    "exposureStatus": "available",
    "attention": "none"
  }],
  "nextCursor": null
}
```

`activityAt` is the terminal lifecycle timestamp when present, otherwise the
submission timestamp. `resolutionAt` is populated only for terminal outcomes.
`reachablePackageCount` is available for completed, completed-with-warnings,
and failed sources that retain a prior snapshot.

`exposureCount: 0` with `exposureStatus: "available"` explicitly means a
completed source has no current KEV-linked exposures. Completed sources with
unavailable CVE or KEV data return a null count and `unavailable`. Pending and
processing sources return `processing`, failed sources return `failed`, and
cancelled sources return `cancelled`; each has a null exposure count. Counts
reflect current vulnerability datasets, not a persisted vulnerability snapshot.

`attention` is `none`, `warnings`, `failed`, or `exposure-data-unavailable`.
A completed-with-warnings source retains usable counts and has `warnings`,
rather than being reported as failed.

### `GET /package-sources/{id}`

Fetch the current status for a submitted package source.

Request:

```sh
curl http://127.0.0.1:8080/package-sources/00000000-0000-0000-0000-000000000001
```

Pending response (durably accepted, awaiting a worker):

```http
HTTP/1.1 200 OK
Content-Type: application/json
```

```json
{
  "status": "pending",
  "id": "00000000-0000-0000-0000-000000000001",
  "createdAt": "2000-01-01T00:00:00Z",
  "attempt": 0
}
```

Processing response:

```http
HTTP/1.1 200 OK
Content-Type: application/json
```

```json
{
  "status": "processing",
  "id": "00000000-0000-0000-0000-000000000001",
  "createdAt": "2000-01-01T00:00:00Z",
  "attempt": 1
}
```

Failed response:

```http
HTTP/1.1 200 OK
Content-Type: application/json
```

```json
{
  "status": "failed",
  "id": "00000000-0000-0000-0000-000000000004",
  "createdAt": "2000-01-01T00:00:00Z",
  "finishedAt": "2000-01-01T00:00:30Z",
  "attempt": 3,
  "diagnostic": "A dependency needed for package-source processing is unavailable.",
  "kind": "dependency-unavailable",
  "retryable": true
}
```

`diagnostic` is always one of a small set of fixed, caller-safe strings keyed
by `kind`; it never contains raw registry, parser, or subprocess error text.
`retryable` reflects whether the automatic-retry service retries failures of
that `kind` at all — it does not mean a retry is currently scheduled, since
the automatic-attempt budget may already be exhausted. See
[Failure and retry](./package-source-lifecycle.md#failure-and-retry) for the
full failure-kind vocabulary and retry rules.

Completed response:

```http
HTTP/1.1 200 OK
Content-Type: application/json
```

```json
{
  "status": "completed",
  "id": "00000000-0000-0000-0000-000000000002",
  "createdAt": "2000-01-01T00:00:00Z",
  "completedAt": "2000-01-01T00:01:00Z",
  "attempt": 1,
  "source": {
    "displayName": "Example application",
    "ecosystem": "npm",
    "fileName": "package.json",
    "contents": "{}"
  },
  "versionedPackages": [
    {
      "id": "00000000-0000-0000-0000-000000000003",
      "name": "react",
      "version": "18.2.0",
      "ecosystem": "npm",
      "purl": "pkg:npm/react@18.2.0",
      "derivations": [
        [
          "<root>",
          "pkg:npm/react@18.2.0"
        ]
      ]
    }
  ]
}
```

Unknown package-source IDs return `404 Not Found`.

```http
HTTP/1.1 404 Not Found
Content-Type: application/json
```

```json
{
  "message": "unknown package source 00000000-0000-0000-0000-000000000099",
  "request_id": "example-request-id"
}
```

Malformed UUID path values are rejected by Dropshot before the handler runs.

### `POST /package-sources/{id}/cancel`

Cancel a package source whose durable state is `pending` or `processing`:

```sh
curl -X POST http://127.0.0.1:8080/package-sources/00000000-0000-0000-0000-000000000001/cancel
```

An accepted or repeated cancellation returns `202 Accepted`:

```json
{
  "id": "00000000-0000-0000-0000-000000000001",
  "status": "cancelled"
}
```

`GET /package-sources/{id}` then returns a `cancelled` status with `createdAt`,
`cancelledAt`, and the attempt number. Cancellation of a source that already
published a terminal result returns `409 Conflict`. Unknown and deleting
sources return `404 Not Found`. Running work polls durable state and stops at a
bounded safe point; generation-guarded publication prevents late work from
overwriting cancellation.

### `DELETE /package-sources/{id}`

Delete submitted source content and all source-scoped versions, edges, and
warnings while preserving canonical package and package-version records:

```sh
curl -X DELETE http://127.0.0.1:8080/package-sources/00000000-0000-0000-0000-000000000001
```

The server first hides the resource in its internal `deleting` state, cancels
publication from any in-flight generation, and completes transactional cleanup.
It returns `202 Accepted` with `{"id":"...","status":"deleting"}`. Reads and
subsequent requests after cleanup return `404 Not Found`. If cleanup was
interrupted after the hide transition, the operator cleanup command recovers
it.

The current MVP has no user or tenant identity model. Every caller that can
reach package-source routes has the same single-operator lifecycle authority;
deployments must restrict route access accordingly. Per-resource authorization
is required before multi-user or external exposure.

### `GET /package-sources/{id}/exposures`

Fetch the KEV-linked reachable exposures for one resolved package source. This
endpoint bridges a completed package-source resolution to Night Vision's
assessment workflow: clients receive each reachable affected package version,
its stored derivation path, the matching CVE identifier, and the linked KEV
context without parsing raw CVE List records.

Request:

```sh
curl http://127.0.0.1:8080/package-sources/00000000-0000-0000-0000-000000000001/exposures
```

Completed response with exposures:

```http
HTTP/1.1 200 OK
Content-Type: application/json
```

```json
{
  "id": "00000000-0000-0000-0000-000000000001",
  "status": "completed",
  "exposures": [
    {
      "package": {
        "id": "00000000-0000-0000-0000-000000000003",
        "name": "left-pad",
        "version": "1.2.3",
        "ecosystem": "npm",
        "purl": "pkg:npm/left-pad@1.2.3",
        "derivations": [
          [
            "<root>",
            "pkg:npm/left-pad@1.2.3"
          ]
        ]
      },
      "cveId": "CVE-2026-1234",
      "kev": {
        "vendorProject": "Example Vendor",
        "product": "left-pad",
        "vulnerabilityName": "Example vulnerability",
        "dateAdded": "2026-08-03"
      }
    }
  ]
}
```

Completed sources that have no reachable KEV-linked exposures still return
`200 OK` with an empty `exposures` array:

```json
{
  "id": "00000000-0000-0000-0000-000000000001",
  "status": "completed",
  "exposures": []
}
```

`completed-with-warnings` sources behave the same way, but return
`"status": "completed-with-warnings"`.

`failed` sources return `200 OK` with `"status": "failed"` and an empty
`exposures` array because no completed exposure snapshot is available.

Processing sources do not yet have a stable exposure result and return
`409 Conflict` with the `PackageSourceExposuresUnavailable` error code.

```http
HTTP/1.1 409 Conflict
Content-Type: application/json
```

```json
{
  "message": "package source exposures are not available until processing completes",
  "error_code": "PackageSourceExposuresUnavailable",
  "request_id": "example-request-id"
}
```

Unknown package-source IDs return `404 Not Found`.

If package resolution is complete but CVE/KEV ingest data is not yet available,
the endpoint returns `503 Service Unavailable` with the
`VulnerabilityDataUnavailable` error code.

```http
HTTP/1.1 503 Service Unavailable
Content-Type: application/json
```

```json
{
  "message": "vulnerability data is not yet available for package source exposures",
  "error_code": "VulnerabilityDataUnavailable",
  "request_id": "example-request-id"
}
```

### `POST /assessments`

Submit a package-version assessment. Supply the affected package version and
the target version as package URLs (PURLs). The server queues the assessment
and returns an ID; the assessment may still be running.

Request:

```sh
curl -X POST http://127.0.0.1:8080/assessments \
  -H 'Content-Type: application/json' \
  -d '{
    "affectedPurl": "pkg:npm/example-package@1.2.3",
    "targetPurl": "pkg:npm/example-package@1.2.4"
  }'
```

Successful response:

```http
HTTP/1.1 202 Accepted
Content-Type: application/json
```

```json
{
  "id": "01990e55-6bae-7aa7-8760-ff4ac5e9d9a8"
}
```

This request is illustrative. A real submission requires the affected PURL to
have a locally known active KEV match, and the target must pass validation
against NPM registry data. Otherwise, the server rejects the request rather
than returning `202 Accepted`.

Use the returned `id` to request the assessment status and evidence. The UUID
above is illustrative; the server generates one for each accepted assessment.

### `GET /assessments/{id}`

Get the current state of an assessment using the ID returned by
`POST /assessments`.

Request:

```sh
curl http://127.0.0.1:8080/assessments/01990e55-6bae-7aa7-8760-ff4ac5e9d9a8
```

Example queued response:

```http
HTTP/1.1 200 OK
Content-Type: application/json
```

```json
{
  "id": "01990e55-6bae-7aa7-8760-ff4ac5e9d9a8",
  "state": "queued",
  "affectedPurl": "pkg:npm/example-package@1.2.3",
  "target": null,
  "sourceRepositoryUrl": null,
  "recommendation": null,
  "findingCount": 0,
  "exitStatus": null,
  "errorKind": null,
  "errorMessage": null,
  "retryable": null
}
```

The `state` changes as assessment work progresses. An unknown ID returns
`404 Not Found`.

### `GET /assessments/{id}/evidence`

Get the checks, findings, and diagnostics recorded for an assessment. Evidence
may be empty while the assessment is queued or running.

Request:

```sh
curl http://127.0.0.1:8080/assessments/01990e55-6bae-7aa7-8760-ff4ac5e9d9a8/evidence
```

Example queued response:

```http
HTTP/1.1 200 OK
Content-Type: application/json
```

```json
{
  "id": "01990e55-6bae-7aa7-8760-ff4ac5e9d9a8",
  "affectedPurl": "pkg:npm/example-package@1.2.3",
  "diagnostics": {
    "sourceRepositoryUrl": null,
    "stdout": null,
    "stdoutTruncated": false,
    "stderr": null,
    "stderrTruncated": false,
    "exitStatus": null,
    "errorKind": null,
    "errorMessage": null,
    "retryable": null
  },
  "checks": [],
  "findings": [],
  "rawHipcheck": null
}
```

An unknown ID returns `404 Not Found`. The optional
`includeRawHipcheck=true` query requests raw Hipcheck output. It requires a
configured diagnostics token and a matching bearer token; without the
configuration, raw access is disabled.

### `POST /upgrade-assessments`

Submit an asynchronous NPM upgrade assessment. The existing request supports
standalone assessments. To connect an assessment to a reachable KEV-linked
exposure in the work queue, include `exposure` with the source ID, reachable
package ID, and CVE ID returned by the queue or exposure endpoint:

```json
{
  "packageSource": {
    "ecosystem": "npm",
    "fileName": "package.json",
    "contents": "{\"name\":\"example-app\",\"version\":\"1.0.0\",\"dependencies\":{\"example-package\":\"1.2.3\"}}"
  },
  "vulnerablePackage": {
    "name": "example-package",
    "ecosystem": "npm",
    "version": "1.2.3"
  },
  "cveLinkage": ["CVE-2026-1234"],
  "candidateVersion": "1.2.7",
  "exposure": {
    "sourceId": "01990e55-6bae-7aa7-8760-ff4ac5e9d9a8",
    "packageId": "00000000-0000-0007-0000-00000000002a",
    "cveId": "CVE-2026-1234"
  }
}
```

The source must have completed resolution. The submitted package-source
contents, vulnerable package name and version, and CVE linkage must match the
referenced exposure. A mismatch returns `400 Bad Request`; unavailable CVE or
KEV data returns an availability error. Accepted submissions return `202` with
`id`, `status`, `createdAt`, and links to the status and result endpoints.
The reference is persisted before analysis starts, so a pending assessment can
appear in the queue. A later assessment for the same exposure becomes its
current linked assessment. Standalone and legacy assessments remain accessible
by ID but cannot be attributed to a submitted source without a validated link.

When `candidateVersion` is omitted, discovery evaluates newer published NPM
versions. Prerelease and deprecated versions are excluded from automatic
discovery. Supplying `candidateVersion` explicitly selects that newer published
version and runs the available candidate analysis. Discovery-only candidates
are available for selection in the frontend; none is server-selected.

### `GET /upgrade-assessments/{id}` and `GET /upgrade-assessments/{id}/result`

The status endpoint returns workflow metadata: `id`, `status`, `createdAt`,
`updatedAt`, and, when applicable, `completedAt` or a bounded error. Status is
`pending`, `completed`, or `failed`. The result endpoint returns the stored
report only after completion; pending or failed results return `409 Conflict`.
The result contains the original input, candidate versions, verdict, findings,
evidence, and caveats. The exposure detail endpoint below is the browser-facing
joined view and omits raw package-source contents.

### `GET /assessment-work-queue`

Read the exposure-backed MVP work queue. Query parameters are `view`
(`needs-review` by default, `processing`, or `completed`), `limit` (default 25,
maximum 50), `cursor`, and `search` (package or source text, maximum 128 bytes).
Rows are traversed in stable internal source order; `nextCursor` is an opaque
continuation value. Continue while it is present, including after a page with no rows for
the selected view or text filter.

Each row identifies its source and, when known, its reachable KEV-linked
package/CVE exposure. A row with `not-assessed` has no assessment ID, candidate,
or verdict and links to its exposure detail. Pending assessments show
`processing`; completed assessments show a selected candidate when one was
explicitly requested, or a candidate preview when discovery produced options.
The preview's `selectionState` remains `available`. Failed assessments and
sources with incomplete resolution or unavailable vulnerability data show
bounded `failed` or `unavailable` states and never a provisional verdict.
Source-status rows link to `/package-sources/{id}`.
Discovery previews prefer `recommended`, then `caution`, `unknown`, and
`avoid`; ties prefer the shorter upgrade distance and then discovery order.

`needs-review` includes unassessed exposures, failed or unavailable rows, and
completed assessments with no candidate, missing candidate evidence, or a
`caution`, `avoid`, or `unknown` candidate verdict. `processing` includes
unfinished source and assessment work. `completed` includes all successfully
finished assessments, so it can overlap with `needs-review`. There is no
reviewed/acknowledged state in this API.

The `coverage` object counts exposures, unassessed exposures, processing
sources, and unavailable sources in the scanned sources for this response. It
is **not a global count** while `nextCursor` is present. `complete` is false
when more sources remain, a source has not finished, or CVE/KEV data is
unavailable. In those cases a zero exposure count must not be interpreted as
proof that no exposures exist. The response includes the same public
`dataStatus` fields as `GET /data-status`, including each dataset's assessment
impact and freshness.

### `GET /assessment-exposures/{source_id}/{package_id}/{cve_id}`

Read one source-scoped exposure and its latest linked assessment. The stable
path key comes from a work-queue exposure row. When data is available, the
response provides source identity, reachable package and dependency paths, CVE
and KEV context, candidates, selected candidate ID, verdict, evidence,
caveats, and data freshness. An exposure with no linked assessment returns
`not-assessed`, an empty candidate list, and no verdict. If source processing
or vulnerability-data availability prevents confirmation, it returns
`unavailable` with an absent exposure instead of implying zero exposure.
Unknown exposures return `404` once the source and vulnerability datasets are
available.

Candidates are paged with `candidateCursor` (zero-based, default 0) and
`candidateLimit` (default 50, maximum 100). Follow `nextCandidateCursor` until
it is absent. The selected candidate ID and top-level verdict describe the
complete assessment even when its candidate is outside the current page.
Reachability is capped at 20 paths of 20 nodes each; `reachabilityTruncated`
marks an omitted path or tail. Candidate findings, evidence, and caveats are
capped at 50 entries each, with corresponding `*Truncated` fields. These caps
keep detail and queue previews bounded without implying that omitted evidence
was absent.

Every candidate has a stable `id` (a canonical NPM PURL when package identity
is known, or an assessment-scoped version key otherwise), version, `upgradeDistance`
(`patch`, `minor`, `major`, or `unknown`),
`semverCompatibility` (`compatible`, `incompatible`, `no-guarantee`, or
`unknown`), `selectionState` (`selected` or `available`), verdict,
`evidenceState`, findings, evidence, caveats, and `majorUpgradeCaution` when
applicable. Only an explicitly requested candidate is selected. Selecting an
available discovery candidate in the frontend changes that page's local
selection; submitting it for analysis creates a new linked assessment.
Candidate-specific findings and evidence appear only for the explicitly
assessed candidate. A discovery candidate without those checks reports
`evidenceState: missing`, even if a vulnerability-based verdict exists.
A major candidate remains selectable but carries the explanation that Night
Vision cannot establish application compatibility. Blocking evidence can still
make its verdict `avoid`. The top-level verdict belongs only to the selected
candidate; it is absent when none is selected.

`candidateState` distinguishes `available`, `no-candidate`, `processing`,
`failed`, and `unavailable`. A completed no-candidate response does not present
an empty recommendation. `dataStatus` explains when stale or unavailable CVE
or KEV data can affect interpretation; stale data does not invalidate recorded
evidence or establish that a package is safe. Night Vision does not prove
application compatibility or calculate BOD 26-04 deadlines.

## Current Package-Source Workflow

The intended package-source workflow is asynchronous:

1. Submit a package source with `POST /package-sources`.
2. Store the returned `id`.
3. Poll `GET /package-sources/{id}` until it reaches a terminal status:
   `completed`, `completed-with-warnings`, `failed`, or `cancelled`.
4. Read `versionedPackages` from a completed response. A
   `completed-with-warnings` response also includes up to 100 structured
   warnings, each with a safe explanatory `message`, and sets
   `warningsTruncated` when more were recorded. A `failed`
   response includes `finishedAt`, the attempt number, a fixed safe
   `diagnostic` string keyed by a structured `kind`, whether that `kind` is
   `retryable`, and retains the last successful graph in
   `previousVersionedPackages`, when one exists.
5. Read `GET /package-sources/{id}/exposures` to retrieve the KEV-linked
   exposure view for that resolved source. A completed source may return
   either exposure entries or an empty `exposures` array; a processing source
   returns `409`, and a completed source returns `503` when local
   vulnerability ingest data is not yet available.

At the API boundary, package sources currently support the `npm` ecosystem and
the `package.json` style of input described in
[Resolving Packages](./resolving-packages.md).

The server persists the submitted source before returning `202 Accepted`. A
durable background dispatcher claims `pending` sources and runs elaboration,
retrying eligible failures automatically with bounded backoff.
`GET /package-sources/{id}` reports the persisted lifecycle state and stored
result snapshot.

For the durable lifecycle covering retries, cancellation, deletion, and
retention, see [Package-Source Lifecycle](./package-source-lifecycle.md).

## Error Responses

Night Vision uses Dropshot error responses. OpenAPI describes error bodies as
JSON objects with these fields:

- `message`: a human-readable error message.
- `request_id`: a request identifier that can be shared with operators.
- `error_code`: an optional machine-readable error code.

See [HTTP Status Codes](./http-status-codes.md) for status-code selection
guidance and framework-generated response behavior.

For `POST /package-sources`, invalid or malformed JSON, missing or unknown
fields, an invalid file name, and invalid manifest contents return `400 Bad
Request`. A non-JSON `Content-Type` returns `415 Unsupported Media Type`.
Oversized manifest contents return `413 Payload Too Large`. If the configured
submission concurrency limit is exhausted, or the configured submission timeout
expires, the endpoint returns `503 Service Unavailable`.

The endpoint has a fixed 3 MiB request-body limit, including JSON encoding.
Dropshot rejects a body exceeding that limit before the handler runs with a
`400 Bad Request`. The process-local concurrency limit protects server capacity;
production ingress must still provide caller-aware rate limiting before the API
is exposed outside a trusted environment.

For `GET /package-sources/{id}/exposures`, unknown IDs return `404 Not Found`.
Processing sources return `409 Conflict` with
`PackageSourceExposuresUnavailable`. Completed sources return `503 Service
Unavailable` with `VulnerabilityDataUnavailable` when the server has not yet
ingested the vulnerability data needed to compute exposure matches. Completed,
completed-with-warnings, and failed package sources otherwise return `200 OK`.

## OpenAPI

The checked-in generated OpenAPI description is:

```text
backend/openapi/nv-server-openapi.json
```

The local sample config sets:

```spookey
openapi-dest-path = "openapi/nv-server-openapi.json"
```

When `openapi-dest-path` is set, `nv-server` writes the OpenAPI description
during startup. To write the OpenAPI description and exit without starting the
server, run `nv-server` with `--openapi` and a config file that sets
`openapi-dest-path`.

```sh
cd backend
cargo run -p nv-server -- --openapi
```

`--openapi` fails if the loaded config does not set `openapi-dest-path`.
