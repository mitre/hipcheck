
# Night Vision Backend

This is the backend server for Night Vision. It is written in Rust, and includes
a REST API to be used by the frontend.

## Backend Crates

- `nv-server`: The actual Night Vision backend server.
- `nv-server-api`: Library crate that defines the `NvServerApi` trait.
- `nvdb`: Night Vision Debugger, a tool for debugging the backend server.
  See the [`nvdb` command reference](../docs/backend/nvdb-command-reference.md)
  for commands, examples, and safety notes.
- `migration`: SeaORM migrations for the PostgreSQL schema. See the
  [database workflow guide](migration/README.md) for adding migrations,
  applying them locally, regenerating entities, and reviewing schema changes.
- `spookey`: Configuration language used by `nv-server`.
- `workspace-hack`: Unifies dependency features, managed by `cargo-hakari`.
- `xtask`: Task runner, used for project-internal tasks.

## Rust Guidelines

The following are guidelines to follow when contributing Rust code to this
backend. These are intended to make it easier to maintain the codebase over
time, including by keeping compile times reasonable.

- __Avoid Procedural Macros if Possible__: Procedural macros are a powerful
  feature when writing Rust, but they interfere with the parallelization of
  Rust builds and can therefore _severely_ negatively impact compilation
  performance. Minimize their usage here. For example, when using `clap` for
  CLI argument parsing, use the builder API, not the derive-based API.
- __Minimize Build Scripts__: especially for intermediate dependencies, if you
  need to introduce a build script, ensure it minimally impacts compilation by
  using the `rerun-if-changed` and `rerun-if-env-changed` directives to avoid
  recompilation when possible. Keep build scripts small and fast to compile.
- __Minimize Monomorphization__: Rust's compile-time generics are powerful, but
  code generation can be slow. Don't lean on compile time guarantees for parts
  of the codebase that don't need it, and pay attention to how new generic code
  impacts compilation performance.

## Database

Night Vision's database of choice is PostgreSQL version 18. Make sure to
install PostgreSQL locally for development. We also recommend installing
pgAdmin if you want a GUI for inspecting the database.

For local backend startup, health checks, logs, PostgreSQL checks, and recovery
steps, see the
[backend operations runbook](../docs/backend/operations-runbook.md).

For migration, entity-generation, local test database, rollback, and schema
review workflows, see the [database workflow guide](migration/README.md).

## Integration Tests

Most backend tests run without external services. Postgres-backed integration
tests are ignored by default. The checked-in integration-test configuration
points to `postgres://localhost:5432/nv_integration_test`. The database name
must contain `test` or `integration` because these tests can clear tables they
own.

From `backend/`, run:

```sh
cargo test -p nv-common cve::integration_tests -- --ignored
```

Set `NV_POSTGRES_INTEGRATION_CONFIG_PATH` to point at another server config file
when your local Postgres setup needs different connection details.

## Secret Configuration

`nv-server` supports database connection secrets through the server
configuration file. See the
[`nv-server` configuration reference](../docs/backend/nv-server-configuration.md)
for the full list of configuration keys, defaults, units, and local/container
behavior.

For local development without credentials, it is acceptable to use
`database-connection` directly:

```spookey
database-connection = "postgres://localhost:5432/nv"
```

For deployed environments, prefer `database-connection-file`:

```spookey
database-connection-file = "/run/secrets/nv-server/database-url"
```

The secret file must contain exactly one line with the full database connection
string. On Unix systems, the file must not grant any group or world
permissions; use mode `0600` or stricter.

Startup config output intentionally redacts both inline secret values and
file-backed secret paths. If secret loading fails, the startup error may name
the secret file path, but it must not print the secret value.
