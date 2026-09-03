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

The API currently has no documented authentication requirement. Do not assume
production authentication, authorization, or deployment behavior from this
local guide.

For local startup, health checks, logs, and recovery steps, see the
[backend operations runbook](./operations-runbook.md).

## JSON Conventions

Request and response bodies use JSON. Field names are camelCase, matching the
OpenAPI schema generated from `backend/nv-server-api/src/lib.rs`.

Current examples include:

- `fileName`
- `createdAt`
- `versionedPackages`

Package-source status responses are tagged by a `status` field. Current status
values are `processing` and `completed`.

## Endpoints

### `GET /health`

Check whether the server can answer API requests.

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
  "status": "ok",
  "cveIngest": {
    "recordsAvailable": false,
    "latestSuccessfulCommit": null,
    "latestRun": null
  }
}
```

`200 OK` means the server answered the request; it does not mean the initial
CVE List sync has completed. `cveIngest.recordsAvailable` is `true` when active
CVE records are available. `latestSuccessfulCommit` and `latestRun` describe
the most recent successful commit and attempted sync, when present.

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
the package-source status.

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
  "createdAt": "2000-01-01T00:00:00Z"
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
      "derivation": [
        "<root>",
        "pkg:npm/react@18.2.0"
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

## Package-Source Lifecycle

The intended package-source workflow is asynchronous:

1. Submit a package source with `POST /package-sources`.
2. Store the returned `id`.
3. Poll `GET /package-sources/{id}` until it reaches a terminal status:
   `completed`, `completed-with-warnings`, or `failed`.
4. Read `versionedPackages` from a completed response. A
   `completed-with-warnings` response also includes up to 100 structured
   warnings and sets `warningsTruncated` when more were recorded. A `failed`
   response includes a diagnostic capped at 1024 UTF-8 bytes.

At the API boundary, package sources currently support the `npm` ecosystem and
the `package.json` style of input described in
[Resolving Packages](./resolving-packages.md).

The server persists the submitted source before returning `202 Accepted` and
runs elaboration in a background task. `GET /package-sources/{id}` reports the
persisted lifecycle state and stored result snapshot.

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
