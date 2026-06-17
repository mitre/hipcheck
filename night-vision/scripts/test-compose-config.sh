#!/bin/sh

# Validate the local Docker Compose configuration with temporary secrets.

set -eu

script_dir=$(
    CDPATH=
    cd -- "$(dirname -- "$0")"
    pwd
)
repo_root=$(
    CDPATH=
    cd -- "$script_dir/.."
    pwd
)

tmp_dir=$(mktemp -d "${TMPDIR:-/tmp}/night-vision-compose-config.XXXXXX")
finish() {
    rm -rf "$tmp_dir"
}
trap finish EXIT HUP INT TERM

printf '%s\n' replace-me > "$tmp_dir/postgres-password"
printf '%s\n' postgres://nv-server:replace-me@postgres:5432/nv > "$tmp_dir/nv-server-database-url"
chmod 600 "$tmp_dir/postgres-password" "$tmp_dir/nv-server-database-url"

cd "$repo_root"

DOCKER_SECRET_MOUNT_DIR="$tmp_dir/docker-mount" \
POSTGRES_PASSWORD_SECRET_FILE="$tmp_dir/postgres-password" \
NV_SERVER_DATABASE_URL_SECRET_FILE="$tmp_dir/nv-server-database-url" \
    scripts/docker-compose-local.sh --env-file .env.local.example config --quiet
