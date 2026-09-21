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

- `fileName`
- `createdAt`
- `versionedPackages`

Package-source status responses are tagged by a `status` field. The current
schemas include `processing`, `completed`, `completed-with-warnings`, and
`failed`, and `cancelled`. Stored `pending` sources use the API's `processing`
compatibility response. Their durable semantics are defined in the
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

Read CVE ingest state for operators. The endpoint is disabled unless
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
    "latestSuccessfulCommit": null,
    "latestRun": null
  }
}
```

`200 OK` means the handler read its diagnostic state; it does not mean the
initial CVE List sync has completed. `cveIngest.recordsAvailable` is `true`
when active CVE records are available. `latestSuccessfulCommit` and `latestRun`
describe the most recent successful commit and attempted sync, when present.
When diagnostics are disabled, the endpoint returns `404 Not Found` without
querying the database.

### `POST /package-sources`

Submit a package source for processing. The endpoint accepts only
`application/json` requests with exactly the `fileName` and `contents` fields.
`fileName` must be exactly `package.json`; paths, alternative file names, and
additional fields are rejected. `contents` must be a valid npm `package.json`
document and must not exceed `package-source-contents-max-bytes` (1 MiB by
default).

Request:

```sh
curl -X POST http://127.0.0.1:8080/package-sources \
  -H 'Content-Type: application/json' \
  -d '{
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

The `202 Accepted` status means the server accepted work that may not be
complete yet. Use the returned `id` with `GET /package-sources/{id}` to check
the package-source status. Cancellation, deletion, and operator-triggered
retention are described in the
[package-source lifecycle](./package-source-lifecycle.md). Durable queueing and
automatic retry are not yet implemented.

### `GET /package-sources/{id}`

Fetch the current status for a submitted package source.

Request:

```sh
curl http://127.0.0.1:8080/package-sources/00000000-0000-0000-0000-000000000001
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
   response includes `finishedAt`, the attempt number, a diagnostic capped at
   1024 UTF-8 bytes, and retains the
   last successful graph in `previousVersionedPackages`, when one exists.

At the API boundary, package sources currently support the `npm` ecosystem and
the `package.json` style of input described in
[Resolving Packages](./resolving-packages.md).

The server persists the submitted source before returning `202 Accepted` and
runs elaboration in a background task. `GET /package-sources/{id}` reports the
persisted lifecycle state and stored result snapshot.

For the durable lifecycle planned for retries, cancellation, deletion, and
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
