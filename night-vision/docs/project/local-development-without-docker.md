# Local Development Without Docker

This guide runs the Night Vision frontend, backend, and PostgreSQL directly on
your workstation. It is intended for contributors who cannot use Docker. The
services bind only to `127.0.0.1`:

| Service | Address |
| --- | --- |
| Frontend | `http://127.0.0.1:3000` |
| Backend | `http://127.0.0.1:8080` |
| PostgreSQL | `127.0.0.1:5432` |

Use this only for a disposable local development database. It is not a
production deployment guide.

## Prerequisites

Install and activate [Flox](./flox.md). The project's Flox environment supplies
PostgreSQL 18, Rust, Node.js, and pnpm. On Windows, run these commands from
WSL; native Windows is not supported by Flox.

From the repository root, activate Flox:

```sh
flox activate
```

Keep the activated shell open while using the commands below. Use separate
Flox-activated terminals for the backend and frontend; use any such terminal
to manage PostgreSQL.

## First-Time Database Setup

The server configuration reads its database URL from an ignored secret file.
Generate the local-only files without printing their contents:

```sh
scripts/setup-compose-secrets.sh -x
```

The script name is retained for compatibility with the Compose workflow; this
command does not start Docker or require a Docker daemon.

Initialize a PostgreSQL cluster in the ignored `.local/postgres` directory.
The explicit `trust` authentication is acceptable only because this cluster is
owned by your user and listens on loopback. Do not use this configuration for a
shared host or any non-local deployment.

```sh
export NV_PGDATA="$PWD/.local/postgres"
mkdir -p "$PWD/.local"
initdb --pgdata "$NV_PGDATA" --auth-host=trust --auth-local=trust
printf "listen_addresses = '127.0.0.1'\n" >> "$NV_PGDATA/postgresql.conf"
pg_ctl --pgdata "$NV_PGDATA" --log "$NV_PGDATA/server.log" --wait start
```

Create the role and database that the default Night Vision configuration uses:

```sh
createuser --host 127.0.0.1 --port 5432 --login nv-server
createdb --host 127.0.0.1 --port 5432 --owner nv-server nv
```

Apply the Night Vision schema migrations. This modifies the local database;
confirm that `nv` is the disposable database you just created.

```sh
cd backend
cargo nvdb db migrate --destructive up
cd ..
```

Run this section only once for a new `.local/postgres` directory. To restart an
existing local database later, use:

```sh
export NV_PGDATA="$PWD/.local/postgres"
pg_ctl --pgdata "$NV_PGDATA" --log "$NV_PGDATA/server.log" --wait start
```

If port 5432 is already in use, stop the conflicting local PostgreSQL service
or configure a different port consistently in the PostgreSQL start command and
the Night Vision database URL configuration. Do not point this setup at a
shared or production database.

## Run the Backend

In a second Flox-activated terminal, start the backend from `backend/`:

```sh
cd backend
cargo run -p nv-server
```

The checked-in `nv-server.spookey` configuration listens on `127.0.0.1:8080`
and reads `.secrets/local-development-database-url`, which the setup command
created. Confirm that the process is responding:

```sh
curl --fail --silent --show-error http://127.0.0.1:8080/health
```

The health endpoint confirms that the server accepts requests. It does not
confirm CVE or KEV data availability. A first CVE List sync can take time and
requires the backend to reach its configured upstream sources.

## Run the Frontend

In a third Flox-activated terminal, configure the host frontend to use the
loopback backend, install dependencies, and start Vite:

```sh
printf 'API_BASE_URL=http://127.0.0.1:8080\n' > frontend/.env
cd frontend
pnpm install
pnpm run dev
```

Open `http://127.0.0.1:3000` in a browser. Vite may select another port if
3000 is occupied; use the address it prints.

## Stop the Services

Stop the frontend and backend with `Ctrl-C` in their terminals. Stop the local
database from the repository root:

```sh
export NV_PGDATA="$PWD/.local/postgres"
pg_ctl --pgdata "$NV_PGDATA" --wait stop
```

The database files remain in `.local/postgres` so that the next `pg_ctl start`
retains your local data. Do not delete that directory unless you intentionally
want to discard all data in this local cluster.

## Related Documentation

- [Frontend README](../../frontend/README.md) for frontend commands and API
  client generation.
- [Backend Operations Runbook](../backend/operations-runbook.md) for backend
  health checks, logs, and recovery.
- [Database Workflow](../../backend/migration/README.md) for migrations and
  generated entities.
- [`nv-server` Configuration](../backend/nv-server-configuration.md) for
  database configuration and secret-file rules.
