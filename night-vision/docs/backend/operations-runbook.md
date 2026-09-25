# Backend Operations Runbook

This runbook covers local `nv-server` startup, health checks, logs, PostgreSQL
checks, common recovery paths, and when to use `nvdb`. It is for contributor
workstations and local Docker Compose, not production incident response.

Run commands from the repository root unless a section says to use `backend/`.
Use an activated Flox environment for normal development commands.

[[_TOC_]]

## Startup

### Direct Local Server

The direct local server uses `backend/nv-server.spookey`. It binds the server
to `127.0.0.1:8080` and connects to a loopback PostgreSQL database. For a
complete Docker-free frontend, backend, and database setup, follow
[Local Development Without Docker](../project/local-development-without-docker.md).
To use the Compose database with a direct local backend, set up and start
Compose before running it:

```sh
scripts/setup-compose-secrets.sh -x
scripts/docker-compose-local.sh up -d postgres
cd backend
cargo run -p nv-server
```

Choose another config file with `--config`:

```sh
cd backend
cargo run -p nv-server -- --config nv-server.spookey
```

`nv-server` writes the OpenAPI description on startup when
`openapi-dest-path` is set. To write OpenAPI and exit without starting the
server:

```sh
cd backend
cargo run -p nv-server -- --openapi
```

For config keys, defaults, and secret-file rules, see
[`nv-server` Configuration](./nv-server-configuration.md).

### Local Docker Compose

Local Compose uses `docker-compose.yml` plus `docker-compose.local.yml`. The
local override binds the backend to `127.0.0.1:8080`, the frontend to
`127.0.0.1:3000`, PostgreSQL to `127.0.0.1:5432`, and the local Postgres
volume to `night-vision-local-postgres-data`.

```sh
cp .env.local.example .env
scripts/setup-compose-secrets.sh -x
scripts/docker-compose-local.sh up --build
```

The Compose wrapper stages local secret files into a Docker-approved host mount
directory before passing them to containers. Do not inspect or paste secret
file contents while debugging. To print resolved file paths without printing
secret values:

```sh
scripts/setup-compose-secrets.sh -p
```

If local Docker cannot mount the default secret staging directory, set
`DOCKER_SECRET_MOUNT_DIR` to a host path Docker is allowed to read.

## Health Checks

Check the direct local server or the Compose-published backend port:

```sh
curl --fail --silent --show-error http://127.0.0.1:8080/health
```

The public liveness response body is:

```json
{
  "status": "ok"
}
```

`GET /health` returning `200 OK` means the server process accepted the request.
It does not query or report database, CVE, or KEV state. Use it for container
and load-balancer liveness checks.

For CVE List and KEV ingest diagnostics, operators can use the bearer-token-protected
endpoint. The local setup script creates the token file with restrictive
permissions:

```sh
curl --fail --silent --show-error http://127.0.0.1:8080/health/diagnostics \
  -H "Authorization: Bearer $(<.secrets/health-diagnostics-token)"
```

Its independent `cveIngest` and `kevIngest` objects report availability,
freshness, the last successful synchronization, and the latest attempt. A new
server can return `200 OK` with `recordsAvailable: false` while its first sync
is still running. A `stale` snapshot remains available but is older than its
configured freshness threshold; a failed latest attempt does not make a prior
snapshot current. Missing or invalid tokens return `401 Unauthorized`; absent
`health-diagnostics-token-file` configuration disables the endpoint with `404
Not Found`. Rotate the token by replacing its secret file and restarting
`nv-server`; do not print or commit token values.

Browser users must use `GET /data-status`, not the diagnostics endpoint. It
returns separate bounded CVE List and CISA KEV summaries without raw error
chains or other operator-only details. A stale KEV summary means newer KEV
records might not appear in assessments; it does not establish that existing
assessment evidence is wrong or that a package is safe. Neither stale nor
unavailable vulnerability data changes the `/health` liveness result.

For Compose, inspect container health and recent logs:

```sh
scripts/docker-compose-local.sh ps
scripts/docker-compose-local.sh logs nv-server
```

The Compose backend healthcheck runs the same local `/health` request inside
the container:

```text
curl --fail --silent --show-error http://127.0.0.1:8080/health >/dev/null
```

If `nv-server` is unhealthy, check Postgres health first because Compose waits
for Postgres before starting the backend.

## Logs

Direct local `nv-server` logs go to the terminal that started the process. The
current logger starts with debug logging disabled and by default uses Dropshot
terminal logging at `Info` level.

On Unix platforms, an operator can toggle debug logging for a running
`nv-server` process without restarting it by sending `SIGUSR1` to the server
process:

```sh
kill -USR1 <pid>
```

The first `SIGUSR1` enables debug log output. A second `SIGUSR1` disables debug
log output again. The toggle state is process-local and resets to disabled when
`nv-server` restarts.

This signal-based toggle is available only on Unix platforms because it depends
on `SIGUSR1`. It is not available for non-Unix builds.

KEV catalog requests emit redacted connection diagnostics. Debug logs record
the sync generation, whether the request is conditional, endpoint scheme, host,
effective port, proxy-variable presence, elapsed time, final endpoint after a
redirect, HTTP version, and response status. Failed requests additionally emit
a warning with a stable failure class (`timeout`, `connect`, `request`, `body`,
`decode`, or `other`), Reqwest category flags, and nested error messages. The
logs intentionally omit request paths, query parameters, headers, proxy values,
and CA material; proxy fields state only whether the conventional environment
variables are configured.

Compose services use Docker's `json-file` logging driver. The base Compose file
limits each service to three 10 MB log files. Read service logs through the
local wrapper:

```sh
scripts/docker-compose-local.sh logs nv-server
scripts/docker-compose-local.sh logs postgres
```

Use request IDs from Dropshot error responses when connecting API failures to
server logs. Avoid shell tracing and broad environment dumps while debugging
because they can expose database URLs, token values, or secret file paths.

## PostgreSQL Checks

For Compose, start with service health:

```sh
scripts/docker-compose-local.sh ps postgres
```

Postgres health uses `pg_isready` with the configured database and user inside
the container. If you need a local host check and your tools can reach the same
database, use placeholder-safe values:

```sh
pg_isready -h 127.0.0.1 -p 5432 -d nv -U nv-server
```

Use `nvdb` when you need checks that read the same Spookey config format as
`nv-server`:

```sh
cd backend
cargo nvdb db migrate --destructive status
cargo nvdb db schema
```

`db migrate --destructive status` still requires `--destructive` because the
command delegates to the migration tool through the same guarded path as
state-changing migration commands. Confirm the config file and target database
before running any `nvdb db migrate` command:

```sh
cd backend
cargo nvdb --config nv-server.spookey db migrate --destructive status
```

For the database development workflow, see
[Database Workflow](../../backend/migration/README.md).

## `nvdb` Usage

`nvdb` is the local backend debugger for database inspection, migrations,
entity generation, and CVE List and KEV catalog operations. Run it from
`backend/`:

```sh
cargo nvdb --help
```

Operational timestamps in `nvdb` text output are UTC by default. Add the
global `--local-time` option to display them in the invoking user's local time
zone with a numeric offset, for example `cargo nvdb kev runs --local-time`.
The option does not change stored values or JSON output, which remains UTC.

Use `nvdb` for:

- Checking migration state with `cargo nvdb db migrate --destructive status`.
- Applying local migrations with `cargo nvdb db migrate --destructive up`.
- Printing the current schema with `cargo nvdb db schema`.
- Regenerating SeaORM entities with `cargo nvdb db entity generate`.
- Inspecting CVE ingest state with `cargo nvdb cve status` and
  `cargo nvdb cve runs`.
- Running one local CVE sync with `cargo nvdb cve sync --destructive`.
- Inspecting KEV cache state with `cargo nvdb kev status` and
  `cargo nvdb kev runs`.
- Running one local KEV sync with `cargo nvdb kev sync --destructive`.

Do not use `nvdb` commands against shared or production-like databases unless
that access and change are explicitly approved. The `--destructive` flag is an
acknowledgement that a command may modify database state; it is not a dry-run
or safety mode.

`nvdb api` provides helpers for health checks, package-source submission and
status, assessment submission, status, and evidence, and upgrade-assessment
submission, status, and results. The helpers use the configured server address and do not
wait for asynchronous work to finish. See [REST API Usage](./rest-api-usage.md)
for endpoint examples.

For full command syntax, expected output, and dependencies such as `pg_dump` and
`sea-orm-cli`, see [`nvdb` Command Reference](./nvdb-command-reference.md).

## CVE List Data

`nv-server` starts a recurring CVE List worker without delaying API startup.
Use health output or these commands to distinguish initial sync, successful
data availability, and failures:

```sh
cd backend
cargo nvdb cve status
cargo nvdb cve runs
cargo nvdb cve stats
```

Run a one-time sync only when intentionally changing local database state:

```sh
cd backend
cargo nvdb cve sync --destructive
```

If an interrupted process leaves a run marked `running`, confirm that no live
sync holds the advisory lock before running `cargo nvdb cve recover
--destructive`. `cve reset --destructive` deletes local CVE List storage and
sync history; use it only for disposable local data. For the full lifecycle,
timeouts, and recovery constraints, see [CVE List Ingest](./cve-ingest.md).

## KEV Catalog Data

`nv-server` refreshes the CISA Known Exploited Vulnerabilities (KEV) catalog
on its configured interval. Inspect the local cache and recent sync history
with:

```sh
cd backend
cargo nvdb kev status
cargo nvdb kev runs
cargo nvdb kev stats
cargo nvdb kev doctor
```

Use `cargo nvdb kev list` to inspect active catalog entries, or
`cargo nvdb kev record <CVE-ID>` to print one stored entry and its local
timestamps. `cargo nvdb kev config` prints the effective source URL, refresh
interval, and response-size limit.

Entries missing from a later successful catalog are retained as history, but do
not affect matching or KEV audits. Run `cargo nvdb kev list --removed` to see
them. Their `removed_at` value is the release time of the catalog snapshot
that first omitted them.

Run a one-time conditional sync only when intentionally changing local
database state:

```sh
cd backend
cargo nvdb kev sync --destructive
```

The sync reuses stored ETag and Last-Modified validators. Pass `--force` only
when an unconditional request is required. `kev pull` remains a deprecated,
hidden alias that prints a warning; use `kev sync` in scripts and runbooks.

If a process exits while a KEV sync run is marked `running`, use
`cargo nvdb kev recover --destructive` only after the command confirms no live
sync holds the advisory lock. Recovery marks abandoned runs failed without
altering cached entries. `cargo nvdb kev reset --destructive` deletes the local
KEV cache and sync history. It refuses a live sync lock and stale `running`
metadata; `--force` overrides only the stale-metadata guard and must not be
used while a sync is live.

## Package-Source Retention Cleanup

Package sources in `completed`, `failed`, or `cancelled` state expire 30 days
after their latest terminal transition. Minimal deletion audits expire after
90 days. The MVP uses an operator-triggered command rather than an in-process
scheduler, so arrange to run it periodically in the deployment's existing job
runner.

Preview one bounded batch without changing the database:

```sh
cd backend
cargo nvdb package-source cleanup --batch-size 100 --dry-run --json
```

Apply the same bounded batch:

```sh
cd backend
cargo nvdb package-source cleanup --batch-size 100 --json
```

`batch-size` defaults to 100 and must be between 1 and 1000. A run scans at
most that many package sources and, independently, at most that many expired
audit rows. The summary reports source rows scanned, deleted, recovered, and
skipped plus audit rows scanned and purged. Dry-run leaves all rows unchanged;
its audit scanned count shows the audit backlog while the purged count remains
zero.

Cleanup first completes sources already hidden in `deleting`, which safely
recovers an interruption between the hide transition and transactional data
removal. It then deletes expired terminal sources through the same path used by
the API. Repeated execution is safe. Run additional batches until both scanned
counts are zero when draining a backlog. A database error makes the command
exit nonzero; correct the database issue and run the same command again.

The command deletes submitted contents and source-scoped versions, edges, and
warnings. It preserves canonical packages and package versions and does not
touch upgrade assessments. It manages live database rows only; deployment
backup and restore policy must enforce the same retention maximum separately.

## Common Failures

### Server Cannot Read Config

`nv-server` and `nvdb` reject unknown keys, missing required keys, invalid
values, and conflicting database connection sources. Compare the file with
[`nv-server` Configuration](./nv-server-configuration.md).

Required local server settings are `server-address` and exactly one of
`database-connection` or `database-connection-file`.

### Database Unavailable

Symptoms include startup failure, migration status failure, or Compose backend
healthcheck failure after Postgres fails health checks.

Check:

```sh
scripts/docker-compose-local.sh ps postgres
scripts/docker-compose-local.sh logs postgres
```

For direct local runs, start the Compose Postgres service and confirm it is
healthy before starting the server.

### Secret File Problems

Secret files must contain exactly one line. On Unix systems, local secret files
outside `/run/secrets/` must not be readable by group or world. Regenerate
local Compose secrets without printing secret values:

```sh
scripts/setup-compose-secrets.sh -x
```

### Changed Compose Database Settings

If `POSTGRES_DB`, `POSTGRES_USER`, or the Postgres password changes after the
local volume has been initialized, recreate the disposable local volume:

```sh
scripts/docker-compose-local.sh down -v
scripts/docker-compose-local.sh up --build
```

This deletes local Compose database state. Do not use this recovery path for
data you need to keep.

### Port Already In Use

The local backend uses `127.0.0.1:8080`. Stop the other process or change the
server config for direct local runs. For Compose, change the local override only
when you need a different published host port.

### Missing External Tools

`cargo nvdb db schema` requires `pg_dump`. Migration and entity-generation
commands require `sea-orm-cli`. Use Flox for the expected local toolchain.

## Before Opening An MR

For docs-only runbook changes, run focused checks:

```sh
cd backend
cargo run -p nv-server -- --help
cargo nvdb --help
cargo nvdb db --help
```

Also review edited Markdown links and examples manually. If backend code,
schema, or command behavior changes, run the relevant backend checks from
`backend/`:

```sh
cargo xtask ci
```

State all checks run in the PR description. If a relevant check was not run,
state why.
