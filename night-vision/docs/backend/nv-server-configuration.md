# `nv-server` Configuration

`nv-server` reads a [Spookey](./spookey-format.md) configuration file at
startup. By default, the server CLI looks for `nv-server.spookey` in the current
directory; use `-c` or `--config` to choose a different file.

The parser is intentionally strict: unknown keys, missing required keys,
invalid values, and conflicting database secret sources stop startup.

For startup commands, health checks, logs, PostgreSQL checks, and local
recovery steps, see the [backend operations runbook](./operations-runbook.md).

## Source Files

- `backend/nv-common/src/config.rs` defines the accepted keys, parses values,
  and maps server settings to Dropshot.
- `backend/nv-common/src/db.rs` applies database pool settings to SeaORM.
- `backend/nv-common/src/rt.rs` applies async runtime settings to Tokio.
- `backend/nv-common/src/secret.rs` loads and validates secret files.
- `backend/nv-server.spookey` is the local development sample.
- `backend/deploy/nv-server.spookey` is packaged into the container image.
- `docker-compose.yml` mounts the container database secret file.

## Required Settings

`server-address` is required. Exactly one database connection source is also
required: configure either `database-connection` or `database-connection-file`,
but not both.

| Key | Value | Local sample | Container sample |
| --- | --- | --- | --- |
| `server-address` | Socket address. | `127.0.0.1:8080` binds loopback only. | `0.0.0.0:8080` binds all container interfaces so Docker publishing and service routing can reach the server. |
| `database-connection` | Database connection string. | Uses `postgres://localhost:5432/nv` for local development without credentials. | Unset. Use file-backed secrets in containers. |
| `database-connection-file` | Path to a file containing the database connection string. | Unset in the local sample. | `/run/secrets/nv-server/database-url`, mounted by Compose. |

## Optional Settings

When an optional key is unset, `nv-server` leaves the downstream Dropshot,
SeaORM, or Tokio setting at its default unless noted below.

| Key | Value | Units | Unset behavior |
| --- | --- | --- | --- |
| `openapi-dest-path` | Path for writing the OpenAPI description on startup. | Path | No OpenAPI file is written. The local sample writes `openapi/nv-server-openapi.json`; the container sample leaves it unset. |
| `http-request-body-max-bytes` | Maximum request body size. | Bytes | Dropshot default, currently 1024 bytes in project comments and sample config. |
| `http-early-disconnect-behavior` | `continue` or `cancel`. | Enum | Dropshot default, which matches `continue`: handlers are detached and run to completion after early disconnect. |
| `database-max-connections` | Maximum database pool connections. | Connections | SeaORM default. The sample config documents 100. |
| `database-min-connections` | Minimum database pool connections. | Connections | SeaORM default. The sample config documents 0. |
| `database-connect-timeout` | Timeout for establishing a database connection. | Milliseconds | No timeout is configured by `nv-server`. |
| `database-idle-timeout` | Timeout for idle database connections. | Milliseconds | No timeout is configured by `nv-server`. |
| `database-acquire-timeout` | Timeout for acquiring a connection from the pool. | Milliseconds | No timeout is configured by `nv-server`. |
| `database-max-lifetime` | Maximum total lifetime for a database connection. | Milliseconds | No timeout is configured by `nv-server`. |
| `async-worker-threads` | Tokio async worker threads. | Threads | Tokio uses the number of CPU cores available to the process. |
| `async-worker-thread-stack-size` | Stack size for worker threads. | Bytes | Tokio default. Project comments describe 2 MB. |
| `async-max-blocking-threads` | Maximum extra blocking threads. | Threads | Tokio default. The sample config documents 512. |
| `async-blocking-thread-keep-alive` | Time to keep idle blocking threads alive. | Milliseconds | Tokio default. The sample config documents 10000 milliseconds. |
| `async-global-queue-interval` | Scheduler ticks between global queue checks. | Ticks | Tokio default. The sample config documents 31 ticks. |
| `async-event-interval` | Scheduler ticks between external event polls. | Ticks | Tokio default. The sample config documents 61 ticks. |

## Secret Files

Use `database-connection-file` when the database connection string contains
credentials. The file must contain exactly one non-empty line with the full
connection string:

```spookey
database-connection-file = "/run/secrets/nv-server/database-url"
```

On Unix systems, secret files outside `/run/secrets/` must not grant group or
world permissions. Use mode `0600` or stricter. Docker Compose secrets mounted
under `/run/secrets/` are accepted with Docker's read-only `0444` mode.

On Windows, `nv-server` rejects secret files readable by broad principals.

Startup configuration output redacts both inline secret values and file-backed
secret paths. If secret loading fails, the startup error may include the secret
file path, but it must not print the secret value.

## Local And Container Behavior

For local development, `backend/nv-server.spookey` uses:

```spookey
server-address = "127.0.0.1:8080"
database-connection = "postgres://localhost:5432/nv"
openapi-dest-path = "openapi/nv-server-openapi.json"
```

For containers, `backend/deploy/nv-server.spookey` uses:

```spookey
server-address = "0.0.0.0:8080"
database-connection-file = "/run/secrets/nv-server/database-url"
```

The base Compose file mounts `NV_SERVER_DATABASE_URL_SECRET_FILE` into the
container as the `nv-server/database-url` secret, which appears to the server
at `/run/secrets/nv-server/database-url`.

## Tuning Notes

- Increase `http-request-body-max-bytes` only when endpoints need larger
  payloads. Larger request bodies increase per-request memory exposure.
- Prefer `http-early-disconnect-behavior = "continue"` for handlers that must
  leave database or in-memory state consistent after clients disconnect.
  `cancel` can reduce wasted work, but request handlers must be cancellation
  safe.
- Keep `database-min-connections` no larger than the steady-state connection
  floor you actually need. Idle minimum connections still consume database
  capacity.
- Keep `database-max-connections` within the Postgres capacity shared by all
  services and developers using the same database.
- Use database timeouts to bound startup or request latency, but choose values
  long enough for local containers and slow development machines.
- Set `async-worker-threads` only when CPU scheduling needs are understood.
  Leaving it unset lets Tokio size the worker pool from available CPU cores.
- `async-max-blocking-threads` limits work offloaded from async workers, such
  as filesystem operations, DNS resolution, and standard I/O. Too low can
  starve blocking work; too high can create excess thread pressure.
- Change `async-global-queue-interval` and `async-event-interval` only for
  measured scheduler behavior. They are low-level Tokio tuning knobs.
