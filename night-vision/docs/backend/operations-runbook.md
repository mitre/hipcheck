# Backend Operations Runbook

This runbook covers local `nv-server` startup, health checks, logs, PostgreSQL
checks, common recovery paths, and when to use `nvdb`. It is for contributor
workstations and local Docker Compose, not production incident response.

Run commands from the repository root unless a section says to use `backend/`.
Use an activated Flox environment for normal development commands.

[[_TOC_]]

## Startup

### Direct Local Server

The direct local server uses `backend/nv-server.spookey`. That sample binds the
server to `127.0.0.1:8080` and uses a credential-free local database URL:

```sh
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
`127.0.0.1:3000`, and the local Postgres volume to
`night-vision-local-postgres-data`.

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

The current health response body is:

```json
{
  "status": "ok"
}
```

`GET /health` returning `200 OK` means the server accepted the request and the
handler completed. It does not currently report database, CVE, or KEV ingest
state.

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
current logger uses Dropshot terminal logging at `Info` level.

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

`nvdb` is the local backend debugger for database inspection, migrations, and
entity generation. Run it from `backend/`:

```sh
cargo nvdb --help
```

Use `nvdb` for:

- Checking migration state with `cargo nvdb db migrate --destructive status`.
- Applying local migrations with `cargo nvdb db migrate --destructive up`.
- Printing the current schema with `cargo nvdb db schema`.
- Regenerating SeaORM entities with `cargo nvdb db entity generate`.

Do not use `nvdb` commands against shared or production-like databases unless
that access and change are explicitly approved. The `--destructive` flag is an
acknowledgement that a command may modify database state; it is not a dry-run
or safety mode.

The current `nvdb` binary lists an `api` placeholder, but REST API helpers are
not implemented there yet. Use [REST API Usage](./rest-api-usage.md) for API
examples.

The current `nvdb` binary does not expose `cve` or `kev` commands. If you need
operational CVE or KEV ingest inspection, file or link a follow-up issue rather
than documenting commands that do not exist. The command reference tracks this
as [issue #57](https://gitlab.mitre.org/night-vision/night-vision/-/work_items/57).

For full command syntax, expected output, and dependencies such as `pg_dump` and
`sea-orm-cli`, see [`nvdb` Command Reference](./nvdb-command-reference.md).

## CVE And KEV Data

The current schema includes tables for CISA KEV entries, CISA KEV sync runs,
CVE List records, and CVE List sync runs. The current server does not start a
CVE or KEV ingest worker, `/health` does not report ingest state, and `nvdb`
does not expose ingest commands.

Practical current states are therefore limited to database state:

- Tables missing: migrations have not been applied to the target database.
- Tables present but empty: migrations exist, but no ingest path has populated
  local data.
- Rows present: data was inserted by local development work, tests, or manual
  database activity outside the current server runtime.

Use these checks to inspect only the local migration state and database shape:

```sh
cd backend
cargo nvdb db migrate --destructive status
cargo nvdb db schema
```

If a feature or issue expects live CVE/KEV ingest state such as `running`,
`success`, `failed`, or `not_modified`, treat that as future behavior until the
server worker and debugger commands exist on the branch you are using.

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

For direct local runs, confirm PostgreSQL 18 is running and the database named
in `backend/nv-server.spookey` exists.

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
