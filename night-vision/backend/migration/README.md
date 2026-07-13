# Database Workflow

Night Vision uses PostgreSQL, SeaORM migrations, and generated SeaORM entities.
This guide covers the normal local workflow for changing the database schema:
add a migration, apply it to a local database, regenerate entities, test the
change, and review the resulting schema.

Run commands from `backend/` inside an activated Flox environment unless a
section says otherwise.

[[_TOC_]]

## Local Database Setup

Night Vision targets PostgreSQL 18. For local development, run a PostgreSQL
database that is safe to modify and point `nv-server.spookey` at it with either
`database-connection` or `database-connection-file`.

For credential-free local development, this is acceptable:

```spookey
database-connection = "postgres://localhost:5432/nv"
```

Do not paste real passwords, secret file contents, or production connection
strings into docs, issues, PR descriptions, or terminal output. If credentials
are needed, prefer `database-connection-file`; see the
[`nv-server` configuration reference](../../docs/backend/nv-server-configuration.md)
for the full secret-handling behavior.

Use a separate local database for tests when a change needs database-backed
verification. The test database should be disposable and should not share state
with a database used for manual exploration. For example, keep one local
database for manual checks and another for tests:

```sh
createdb nv
createdb nv_test
```

Point each workflow at the intended database with its own Spookey config or
`DATABASE_URL` value, then apply the migrations expected by the test before
running database-backed checks. Never aim destructive migration commands at a
database whose state you still need.

## Adding a Migration

Generate migrations from the `migration` crate:

```sh
cargo run --package migration -- generate <migration_name>
```

SeaORM creates a new file under `backend/migration/src/`. Use a descriptive
snake_case name that captures the schema change, such as
`add_package_indexes`.

After generating the file:

- Implement both `up` and `down` when the change can be reversed safely.
- Add the migration module to `backend/migration/src/lib.rs`.
- Add the migration to `Migrator::migrations()` in the order it should run.
- Review table names, column types, nullability, indexes, constraints, and
  foreign keys before applying the migration.
- Keep data-loss or compatibility-sensitive changes obvious in the migration
  code and PR description.

## Applying Migrations Locally

Prefer `nvdb db migrate` for normal local development because it uses the same
Spookey configuration format as `nv-server`:

```sh
cargo nvdb db migrate --destructive status
```

```sh
cargo nvdb db migrate --destructive up
```

```sh
cargo nvdb db migrate --destructive up -n 2
```

The `--destructive` flag is an acknowledgement that the command may modify
database state. It is not a dry-run mode. Before running a migration, confirm
which config file and database are targeted:

```sh
cargo nvdb --config nv-server.spookey db migrate --destructive status
```

Do not run local migration commands against shared or production-like databases
unless that access and change are explicitly approved.

Direct SeaORM migration commands are also available from the backend workspace.
They read `DATABASE_URL` and are useful when debugging SeaORM migration behavior
itself:

```sh
cargo run --package migration -- status
```

```sh
cargo run --package migration -- up
```

```sh
cargo run --package migration -- down
```

For more migration subcommands, run:

```sh
cargo run --package migration -- --help
```

## Generating SeaORM Entities

After applying migrations to the local database, regenerate SeaORM entities:

```sh
cargo nvdb db entity generate
```

The command runs `sea-orm-cli generate entity` against the configured database
and writes generated files under:

```text
backend/nv-common/src/db/entities
```

It ignores the `seaql_migrations` table and uses the `chrono` date-time crate.
Because entity output reflects the connected database, confirm the database has
the migrations expected for the PR before committing generated files.

Review generated diffs carefully. Entity changes should match the migration's
schema intent and should not include unrelated schema drift from a stale or
misconfigured local database.

## Rollback Policy

Every migration should include a meaningful `down` path when rollback is safe
and practical. Use rollback locally to confirm reversible changes:

```sh
cargo nvdb db migrate --destructive down
```

If a schema change is not safely reversible, call that out in the migration
review and PR description. Destructive local reset flows are available, but use
them only on disposable local databases:

```sh
cargo run --package migration -- fresh
```

```sh
cargo run --package migration -- refresh
```

```sh
cargo run --package migration -- reset
```

Do not use `fresh`, `refresh`, or `reset` against databases that contain data
you need to keep.

## Schema Review

Before opening an MR, inspect the schema produced by the migration:

```sh
cargo nvdb db schema
```

Review the schema and generated entity diffs for:

- Tables, columns, indexes, constraints, and foreign keys.
- Nullability, defaults, uniqueness, and data-loss risk.
- Compatibility with existing server code and API behavior.
- Whether generated entity changes match only the intended schema changes.
- Whether any rollout or rollback concern needs reviewer attention.

If the docs or tools do not describe behavior needed for the change, file a
follow-up issue instead of documenting commands or guarantees that do not
exist.

## Testing

For docs-only migration workflow changes, manually check commands against the
current help output and review links. If schema, migration, entity, or backend
code changes are included, run the relevant backend checks from `backend/`:

```sh
cargo xtask ci
```

If a full check cannot be run, state that in the PR description with the reason
and list the focused checks that did run.
