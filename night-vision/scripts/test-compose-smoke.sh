#!/bin/sh

# Build and smoke-test the Docker Compose stack with temporary secrets.

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

tmp_parent_dir=${DOCKER_COMPOSE_TMPDIR:-"$repo_root/compose-smoke-secrets"}
mkdir -p "$tmp_parent_dir"
tmp_dir=$(mktemp -d "$tmp_parent_dir/night-vision-compose-smoke.XXXXXX")
chmod 755 "$tmp_dir"
compose_project_name=night-vision-ci-${CI_JOB_ID:-local}

finish() {
    status=$?

    if [ "$status" -ne 0 ]; then
        COMPOSE_PROJECT_NAME="$compose_project_name" \
        BUILD_CA_FILE="${BUILD_CA_FILE:-/dev/null}" \
        NV_APP_IMAGE="${NV_APP_IMAGE:-nv-app:ci-smoke}" \
        NV_SERVER_IMAGE="${NV_SERVER_IMAGE:-nv-server:ci-smoke}" \
        POSTGRES_DB=nv \
        POSTGRES_USER=nv-server \
        POSTGRES_PASSWORD_SECRET_FILE="$tmp_dir/postgres-password" \
        NV_SERVER_DATABASE_URL_SECRET_FILE="$tmp_dir/nv-server-database-url" \
            docker compose \
                -f "$repo_root/docker-compose.yml" \
                -f "$repo_root/docker-compose.ci.yml" \
                ps || true

        COMPOSE_PROJECT_NAME="$compose_project_name" \
        BUILD_CA_FILE="${BUILD_CA_FILE:-/dev/null}" \
        NV_APP_IMAGE="${NV_APP_IMAGE:-nv-app:ci-smoke}" \
        NV_SERVER_IMAGE="${NV_SERVER_IMAGE:-nv-server:ci-smoke}" \
        POSTGRES_DB=nv \
        POSTGRES_USER=nv-server \
        POSTGRES_PASSWORD_SECRET_FILE="$tmp_dir/postgres-password" \
        NV_SERVER_DATABASE_URL_SECRET_FILE="$tmp_dir/nv-server-database-url" \
            docker compose \
                -f "$repo_root/docker-compose.yml" \
                -f "$repo_root/docker-compose.ci.yml" \
                logs --no-color nv-server postgres || true
    fi

    COMPOSE_PROJECT_NAME="$compose_project_name" \
    BUILD_CA_FILE="${BUILD_CA_FILE:-/dev/null}" \
    NV_APP_IMAGE="${NV_APP_IMAGE:-nv-app:ci-smoke}" \
    NV_SERVER_IMAGE="${NV_SERVER_IMAGE:-nv-server:ci-smoke}" \
    POSTGRES_DB=nv \
    POSTGRES_USER=nv-server \
    POSTGRES_PASSWORD_SECRET_FILE="$tmp_dir/postgres-password" \
    NV_SERVER_DATABASE_URL_SECRET_FILE="$tmp_dir/nv-server-database-url" \
        docker compose \
            -f "$repo_root/docker-compose.yml" \
            -f "$repo_root/docker-compose.ci.yml" \
            down -v --remove-orphans >/dev/null 2>&1 || true

    rm -rf "$tmp_dir"
    exit "$status"
}
trap finish EXIT HUP INT TERM



printf '%s\n' replace-me > "$tmp_dir/postgres-password"
printf '%s\n' postgres://nv-server:replace-me@postgres:5432/nv > "$tmp_dir/nv-server-database-url"
printf '%s\n' ci-health-diagnostics-token > "$tmp_dir/health-diagnostics-token"
# Compose file-backed secrets are bind-mounted into containers by Docker
# Compose. Make the smoke-test fixtures readable by the non-root container
# users while matching Docker's default /run/secrets file mode.
chmod 444 "$tmp_dir/postgres-password" "$tmp_dir/nv-server-database-url" "$tmp_dir/health-diagnostics-token"
export HEALTH_DIAGNOSTICS_TOKEN_SECRET_FILE="$tmp_dir/health-diagnostics-token"

cd "$repo_root"

COMPOSE_PROJECT_NAME="$compose_project_name" \
BUILD_CA_FILE="${BUILD_CA_FILE:-/dev/null}" \
NV_APP_IMAGE="${NV_APP_IMAGE:-nv-app:ci-smoke}" \
NV_SERVER_IMAGE="${NV_SERVER_IMAGE:-nv-server:ci-smoke}" \
POSTGRES_DB=nv \
POSTGRES_USER=nv-server \
POSTGRES_PASSWORD_SECRET_FILE="$tmp_dir/postgres-password" \
NV_SERVER_DATABASE_URL_SECRET_FILE="$tmp_dir/nv-server-database-url" \
    docker compose \
        -f docker-compose.yml \
        -f docker-compose.ci.yml \
        build nv-server

printf 'Backend runtime Git version: '
docker run \
    --rm \
    --entrypoint git \
    "${NV_SERVER_IMAGE:-nv-server:ci-smoke}" \
    --version

hipcheck_revision=$(docker run \
    --rm \
    --entrypoint cat \
    "${NV_SERVER_IMAGE:-nv-server:ci-smoke}" \
    /opt/night-vision/hipcheck/REVISION
)
printf 'Bundled Hipcheck revision: %s\n' "$hipcheck_revision"
printf '%s' "$hipcheck_revision" | grep -Eq '^[0-9a-f]{40}$'

COMPOSE_PROJECT_NAME="$compose_project_name" \
BUILD_CA_FILE="${BUILD_CA_FILE:-/dev/null}" \
NV_APP_IMAGE="${NV_APP_IMAGE:-nv-app:ci-smoke}" \
NV_SERVER_IMAGE="${NV_SERVER_IMAGE:-nv-server:ci-smoke}" \
POSTGRES_DB=nv \
POSTGRES_USER=nv-server \
POSTGRES_PASSWORD_SECRET_FILE="$tmp_dir/postgres-password" \
NV_SERVER_DATABASE_URL_SECRET_FILE="$tmp_dir/nv-server-database-url" \
    docker compose \
        -f docker-compose.yml \
        -f docker-compose.ci.yml \
        up --build --detach --wait --wait-timeout 180

COMPOSE_PROJECT_NAME="$compose_project_name" \
BUILD_CA_FILE="${BUILD_CA_FILE:-/dev/null}" \
NV_APP_IMAGE="${NV_APP_IMAGE:-nv-app:ci-smoke}" \
NV_SERVER_IMAGE="${NV_SERVER_IMAGE:-nv-server:ci-smoke}" \
POSTGRES_DB=nv \
POSTGRES_USER=nv-server \
POSTGRES_PASSWORD_SECRET_FILE="$tmp_dir/postgres-password" \
NV_SERVER_DATABASE_URL_SECRET_FILE="$tmp_dir/nv-server-database-url" \
    docker compose \
        -f docker-compose.yml \
        -f docker-compose.ci.yml \
        exec -T nv-server \
        /usr/local/bin/hc \
        --policy /opt/night-vision/hipcheck/config/Hipcheck.kdl \
        --exec /opt/night-vision/hipcheck/config/Exec.kdl \
        --cache /var/cache/night-vision/hipcheck \
        ready
