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

The package-source API currently has no user or tenant identity model. Treat
access to it as single-operator authority and do not assume production
authentication, authorization, or deployment behavior from this local guide.

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

Upgrade-assessment status responses are also tagged by `status`. Their values
are `processing`, `completed`, and `failed`.

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

### `POST /upgrade-assessments`

Request an asynchronous assessment of an NPM package version. A trigger is
either the CVE that prompted the assessment or a previously-created exposure
identifier. `candidateVersion` is optional: clients can use it to request a
specific upgrade candidate.

```sh
curl -X POST http://127.0.0.1:8080/upgrade-assessments \
  -H 'Content-Type: application/json' \
  -d '{
    "packageName": "example-package",
    "currentVersion": "1.2.3",
    "trigger": { "kind": "cve", "cve_id": "CVE-2026-1234" },
    "candidateVersion": "1.2.7"
  }'
```

The successful `202 Accepted` response returns a unique assessment ID. The
request is persisted before in-process evaluation begins, so clients may poll
it immediately.

```json
{ "id": "01990e55-6bae-7aa7-8760-ff4ac5e9d9a8" }
```

Empty package names, versions, or trigger references return `400 Bad Request`.
CVE triggers must use a `CVE-YYYY-NNNN`-style identifier in `trigger.cve_id`.

### `GET /upgrade-assessments/{id}`

Retrieve a persisted assessment. While work is pending, the response includes
the normalized NPM input:

```json
{
  "status": "processing",
  "id": "01990e55-6bae-7aa7-8760-ff4ac5e9d9a8",
  "createdAt": "2026-09-01T12:00:00Z",
  "input": {
    "ecosystem": "npm",
    "packageName": "example-package",
    "currentVersion": "1.2.3",
    "trigger": { "kind": "cve", "cve_id": "CVE-2026-1234" },
    "candidateVersion": "1.2.7"
  }
}
```

A completed report includes the input, verdict, candidate versions,
vulnerability/KEV context, dependency delta, supply-chain findings,
confidence, caveats, and evidence links. When `candidateVersion` is supplied,
Night Vision validates the candidate against locally known KEV data, runs
Hipcheck, and uses normalized Hipcheck findings as supply-chain evidence for
the verdict. Hipcheck's `PASS` or `INVESTIGATE` policy recommendation is shown
as context; it does not replace vulnerability or upgrade-domain evidence.
`vulnerabilityContext.kevLinked` describes whether the affected baseline
package version is linked to an active KEV entry; it does not describe the
candidate verdict. Every accepted upgrade assessment currently has
`kevLinked: true`, because the baseline KEV match is required before work is
started.
When `candidateVersion` is omitted, Night Vision fetches the NPM packument and
evaluates every newer, published, non-prerelease and non-deprecated version in
ascending SemVer order. It records each candidate's patch/minor/major distance,
SemVer compatibility, verdict, and KEV-evidence caveat in the completed report.
Candidates that still match a locally known active KEV vulnerability are
`avoid`; candidates without such a match are `recommended` unless SemVer
provides no compatibility guarantee. Major candidates, and candidates in the
`0.y.z` or prerelease compatibility domain, are at least `caution` because
Night Vision cannot establish application compatibility. Blocking KEV evidence
continues to produce `avoid`. Pre-release and deprecated releases are excluded
from automatic discovery. An explicit published newer patch, minor, or major
version is preserved as the single assessment target and evaluated with the
same compatibility verdict rules plus Hipcheck evidence.

When Hipcheck analysis is available, the shipped MVP policy provides only the
`mitre/binary` source-repository check. NPM release, artifact, maintainer,
provenance, manifest, publication, and dependency signal classes remain
unavailable. Clients must read those limitations from missing-evidence findings
or caveats; a missing finding does not mean an unavailable check passed.

```json
{
  "status": "completed",
  "id": "01990e55-6bae-7aa7-8760-ff4ac5e9d9a8",
  "createdAt": "2026-09-01T12:00:00Z",
  "completedAt": "2026-09-01T12:00:01Z",
  "report": {
    "input": {
      "ecosystem": "npm",
      "packageName": "example-package",
      "currentVersion": "1.2.3",
      "trigger": { "kind": "cve", "cve_id": "CVE-2026-1234" }
    },
    "verdict": "recommended",
    "candidateVersions": [{
      "version": "1.2.7",
      "upgradeDistance": "patch",
      "apiCompatibility": "compatible",
      "verdict": "recommended",
      "caveats": ["Candidate has no locally known active KEV vulnerability."]
    }],
    "vulnerabilityContext": { "trigger": { "kind": "cve", "cve_id": "CVE-2026-1234" }, "kevLinked": true },
    "dependencyDelta": { "added": [], "removed": [], "changed": [] },
    "supplyChainFindings": [],
    "confidence": "medium",
    "caveats": ["Candidates are ordered by ascending SemVer version. Pre-release and deprecated releases are excluded from automatic discovery."],
    "evidenceLinks": []
  }
}
```

If evaluation fails after acceptance, `status` is `failed` and the response
contains `failedAt`, the original input, and a non-secret error message. An
unknown ID returns `404 Not Found`.

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
