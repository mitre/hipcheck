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

Submit a package source for processing. The current API accepts the source file
name and the complete source file contents.

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
3. Poll `GET /package-sources/{id}` until the status is `completed`.
4. Read `versionedPackages` from the completed response.

At the API boundary, package sources currently support the `npm` ecosystem and
the `package.json` style of input described in
[Resolving Packages](./resolving-packages.md).

The current backend handler still uses placeholder package-source storage and
lookup behavior. `POST /package-sources` returns a fixed example UUID, and
`GET /package-sources/{id}` returns hardcoded example statuses for known
placeholder IDs. Treat these examples as the current local API contract, not as
evidence of completed persistence or package resolution.

## Error Responses

Night Vision uses Dropshot error responses. OpenAPI describes error bodies as
JSON objects with these fields:

- `message`: a human-readable error message.
- `request_id`: a request identifier that can be shared with operators.
- `error_code`: an optional machine-readable error code.

See [HTTP Status Codes](./http-status-codes.md) for status-code selection
guidance and framework-generated response behavior.

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
