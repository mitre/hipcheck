# `nvdb` Command Reference

`nvdb` is the Night Vision backend debugger. Use it for local backend
development tasks that need the same configuration format as `nv-server`,
especially database inspection, migration, and SeaORM entity maintenance.

This reference is based on the current `nvdb --help` output. If the help text
does not describe the behavior needed for a task, update the CLI or file a
follow-up issue instead of documenting intended commands that do not exist.

[[_TOC_]]

## Running `nvdb`

Run `nvdb` from `backend/` inside an activated Flox environment:

```sh
cargo nvdb --help
```

`backend/.cargo/config.toml` defines `cargo nvdb` as a project-specific Cargo
alias for `cargo run --package nvdb --quiet --`. For example:

```sh
cargo nvdb db schema
```

The expanded Cargo command also works:

```sh
cargo run -p nvdb -- db schema
```

When installed or run directly, omit the Cargo wrapper:

```sh
nvdb db schema
```

## Configuration

`nvdb` accepts the same Spookey configuration file format used by `nv-server`.
By default, it reads `nv-server.spookey` from the current directory.

```text
Usage: nvdb [OPTIONS] [COMMAND]

Options:
  -c, --config <FILE>  Path to the configuration file [default: nv-server.spookey]
  -h, --help           Print help
```

Use `--config` when the config file is not the default local sample:

```sh
cargo nvdb --config nv-server.spookey db schema
```

The configuration must provide exactly one database connection source:
`database-connection` or `database-connection-file`. See
[`nv-server` Configuration](./nv-server-configuration.md) for the full
configuration reference.

## Secret Safety

Do not paste real database passwords, tokens, or secret file contents into
examples, issue comments, PR descriptions, or terminal output.

Prefer file-backed secrets when credentials are required:

```spookey
database-connection-file = "/run/secrets/nv-server/database-url"
```

Local examples may use placeholder or credential-free connection strings:

```spookey
database-connection = "postgres://localhost:5432/nv"
```

`nvdb` uses environment variables when calling tools such as `pg_dump` and
`sea-orm-cli`, so database connection strings are not passed as command-line
arguments. Still avoid printing process environments or shell traces when
working with real credentials.

## Commands

Current top-level commands:

```text
Commands:
  api   Interact with the REST API
  db    Manage the database
  help  Print this message or the help of the given subcommand(s)
```

The `api` command is listed in help output, but it is not implemented yet.
Running it currently reaches a placeholder in the binary. Use the
[REST API Usage](./rest-api-usage.md) guide for current API examples.

The issue tracker mentions `cve` command coverage, but current `nvdb` help does
not expose a `cve` subcommand. `nvdb cve --help` exits with an unrecognized
subcommand error. Do not document `cve` command behavior until the command is
implemented. Follow-up issue
[#57](https://gitlab.mitre.org/night-vision/night-vision/-/work_items/57)
tracks that implementation decision.

## `db`

Use `db` commands for database inspection and maintenance:

```text
Usage: nvdb db [OPTIONS] [COMMAND]

Commands:
  entity   Manage SeaORM entities
  schema   Print the current database schema
  migrate  Run database migrations
  help     Print this message or the help of the given subcommand(s)

Options:
  -c, --config <FILE>  Path to the configuration file [default: nv-server.spookey]
  -h, --help           Print help
```

### `db schema`

Print the current database schema:

```text
Usage: nvdb db schema [OPTIONS]

Options:
  -c, --config <FILE>  Path to the configuration file [default: nv-server.spookey]
  -h, --help           Print help
```

Example:

```sh
cargo nvdb db schema
```

Expected output is SQL schema text from `pg_dump --schema-only --no-owner
--no-privileges`. The output can include table, index, constraint, and migration
metadata definitions. It should not include table data.

`db schema` requires `pg_dump` to be available and a database connection that
the configured user can read.

### `db entity generate`

Generate SeaORM entity source files from the current database schema:

```text
Usage: nvdb db entity [OPTIONS] [COMMAND]

Commands:
  generate  Generate SeaORM entities
  help      Print this message or the help of the given subcommand(s)

Options:
  -c, --config <FILE>  Path to the configuration file [default: nv-server.spookey]
  -h, --help           Print help
```

```text
Usage: nvdb db entity generate [OPTIONS]

Options:
  -c, --config <FILE>  Path to the configuration file [default: nv-server.spookey]
  -h, --help           Print help
```

Example:

```sh
cargo nvdb db entity generate
```

Expected output is produced by `sea-orm-cli generate entity`. The command writes
generated entity files under `backend/nv-common/src/db/entities`, ignoring the
`seaql_migrations` table and using the `chrono` date-time crate.

Review generated files carefully before committing them. Entity output reflects
the connected database schema, so make sure the target database has the
migrations expected for the change under review.

### `db migrate`

Run database migrations through `sea-orm-cli migrate`:

```text
Usage: nvdb db migrate [OPTIONS] --destructive [ARGS]...

Arguments:
  [ARGS]...  Arguments to pass to sea-orm-cli migrate

Options:
  -c, --config <FILE>  Path to the configuration file [default: nv-server.spookey]
  -w, --destructive    Acknowledge this command may modify database state
  -h, --help           Print help
```

`db migrate` requires `--destructive` because migrations may change database
state. The flag is an explicit acknowledgement; it is not a dry-run or safety
mode.

Examples:

```sh
cargo nvdb db migrate --destructive up
```

```sh
cargo nvdb db migrate --destructive up -n 2
```

```sh
cargo nvdb --config nv-server.spookey db migrate --destructive status
```

Expected output is whatever `sea-orm-cli migrate` prints for the forwarded
arguments, such as migration status or applied migration messages.

Before running `db migrate`, confirm which config file and database you are
targeting. Do not run migrations against shared or production-like databases
from a local checkout unless that access and change are explicitly approved.

## Troubleshooting

If `nvdb` cannot read configuration, compare the config file with
[`nv-server` Configuration](./nv-server-configuration.md). Unknown keys,
missing required keys, invalid values, or conflicting database connection
sources stop startup.

If `db schema` fails, check that `pg_dump` is installed, the configured database
is reachable, and the database user can inspect the schema.

If `db entity generate` or `db migrate` fails, check that `sea-orm-cli` is
available in the active environment and that the configured database connection
is valid.

If a command appears in issue scope but not in `nvdb --help`, treat that as an
implementation gap. File or link a follow-up issue rather than documenting
nonexistent behavior.
