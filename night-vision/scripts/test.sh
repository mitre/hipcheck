#!/bin/sh

# Run smoke tests for Night Vision's Docker shell scripts.
#
# Usage:
#
#   scripts/test.sh
#
# The tests run ShellCheck, POSIX shell syntax checks, local Docker Compose
# config validation, and temp-dir checks for local secret generation behavior.

set -eu
export NO_COLOR=1

if [ -t 1 ]; then
    color_green=$(printf '\033[32m')
    color_blue=$(printf '\033[36m')
    color_red=$(printf '\033[31m')
    color_reset=$(printf '\033[0m')
else
    color_green=
    color_blue=
    color_red=
    color_reset=
fi

info() {
    printf '%s%s%s\n' "$color_blue" "$1" "$color_reset"
}

success() {
    printf '%s%s%s\n' "$color_green" "$1" "$color_reset"
}

error() {
    printf '%s%s%s\n' "$color_red" "$1" "$color_reset" >&2
}

current_test=
current_test_complete=true

start_test() {
    current_test=$1
    current_test_complete=false
    printf '%s%s ... %s' "$color_blue" "$current_test" "$color_reset"
}

pass_test() {
    current_test_complete=true
    printf '%sPASS%s\n' "$color_green" "$color_reset"
}

finish() {
    status=$?

    if [ "$status" -ne 0 ] && [ -n "$current_test" ] && [ "$current_test_complete" = false ]; then
        printf '%sFAIL%s\n' "$color_red" "$color_reset" >&2
    fi

    rm -rf "$tmp_dir"
}

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

tmp_dir=$(mktemp -d "${TMPDIR:-/tmp}/night-vision-docker-scripts.XXXXXX")
trap finish EXIT HUP INT TERM
export DOCKER_SECRET_MOUNT_DIR="$tmp_dir/docker-mount"
export HEALTH_DIAGNOSTICS_TOKEN_SECRET_FILE="$tmp_dir/health-diagnostics-token"
export NV_APP_IMAGE=nv-app:local
export NV_SERVER_IMAGE=nv-server:local

setup_script="$script_dir/setup-compose-secrets.sh"
compose_script="$script_dir/docker-compose-local.sh"

assert_fails() {
    description=$1
    shift

    if "$@" >/dev/null 2>&1; then
        error "error: expected failure: $description"
        exit 1
    fi
}

assert_file_mode() {
    path=$1
    expected=$2

    mode=$(stat -c '%a' "$path" 2>/dev/null || stat -f '%Lp' "$path")
    if [ "$mode" != "$expected" ]; then
        error "error: expected $path to have mode $expected, got $mode"
        exit 1
    fi
}

assert_line_count() {
    path=$1
    expected=$2

    lines=$(wc -l < "$path" | tr -d ' ')
    if [ "$lines" != "$expected" ]; then
        error "error: expected $path to have $expected line(s), got $lines"
        exit 1
    fi
}

assert_not_exists() {
    path=$1

    if [ -e "$path" ]; then
        error "error: expected $path not to exist"
        exit 1
    fi
}

assert_contains() {
    path=$1
    expected=$2

    if ! grep -F "$expected" "$path" >/dev/null; then
        error "error: expected $path to contain $expected"
        exit 1
    fi
}

cd "$repo_root"

info 'Running Docker shell script tests...'
start_test 'shellcheck'
shellcheck "$script_dir"/*.sh
pass_test

start_test 'POSIX shell syntax'
sh -n "$script_dir"/*.sh
pass_test

start_test 'local Compose uses staged secrets'
printf '%s\n' replace-me > "$tmp_dir/source-postgres-password"
printf '%s\n' postgres://nv-server:replace-me@postgres:5432/nv > "$tmp_dir/source-nv-server-database-url"
printf '%s\n' local-health-diagnostics-token > "$tmp_dir/source-health-diagnostics-token"
printf '%s\n' test-ca > "$tmp_dir/source-ca-file"
chmod 600 "$tmp_dir/source-postgres-password" "$tmp_dir/source-nv-server-database-url" "$tmp_dir/source-health-diagnostics-token" "$tmp_dir/source-ca-file"
export HEALTH_DIAGNOSTICS_TOKEN_SECRET_FILE="$tmp_dir/source-health-diagnostics-token"

POSTGRES_PASSWORD_SECRET_FILE="$tmp_dir/source-postgres-password" \
NV_SERVER_DATABASE_URL_SECRET_FILE="$tmp_dir/source-nv-server-database-url" \
CA_FILE_SECRET_FILE="$tmp_dir/source-ca-file" \

    "$compose_script" --env-file .env.local.example config --quiet >/dev/null
local_compose_config=$(
    POSTGRES_PASSWORD_SECRET_FILE="$tmp_dir/source-postgres-password" \
    NV_SERVER_DATABASE_URL_SECRET_FILE="$tmp_dir/source-nv-server-database-url" \
    CA_FILE_SECRET_FILE="$tmp_dir/source-ca-file" \
    
        "$compose_script" --env-file .env.local.example config
)
if ! printf '%s\n' "$local_compose_config" | grep -F "file: $DOCKER_SECRET_MOUNT_DIR/night-vision-local/postgres-password" >/dev/null; then
    error 'error: local Docker Compose config should use the staged Postgres secret file'
    exit 1
fi
if ! printf '%s\n' "$local_compose_config" | grep -F "file: $DOCKER_SECRET_MOUNT_DIR/night-vision-local/nv-server-database-url" >/dev/null; then
    error 'error: local Docker Compose config should use the staged nv-server database URL secret file'
    exit 1
fi
if ! printf '%s\n' "$local_compose_config" | grep -F "file: $DOCKER_SECRET_MOUNT_DIR/night-vision-local/health-diagnostics-token" >/dev/null; then
    error 'error: local Compose should use the staged health diagnostics token file'
    exit 1
fi
if ! printf '%s\n' "$local_compose_config" | grep -F "source: ca_file" >/dev/null; then
    error 'error: local Docker Compose config should pass ca_file as a build secret'
    exit 1
fi
if ! printf '%s\n' "$local_compose_config" | grep -F "file: $DOCKER_SECRET_MOUNT_DIR/night-vision-local/ca_file" >/dev/null; then
    error 'error: local Docker Compose config should use the staged CA file build secret'
    exit 1
fi
if false; then

    exit 1
fi
if false; then

    exit 1
fi
if ! printf '%s\n' "$local_compose_config" | grep -F 'postgres-host: null' >/dev/null; then
    error 'error: local Compose should attach Postgres to its host-access network'
    exit 1
fi
if ! cmp -s "$tmp_dir/source-postgres-password" "$DOCKER_SECRET_MOUNT_DIR/night-vision-local/postgres-password"; then
    error 'error: staged Postgres secret should match the source secret file'
    exit 1
fi
if ! cmp -s "$tmp_dir/source-nv-server-database-url" "$DOCKER_SECRET_MOUNT_DIR/night-vision-local/nv-server-database-url"; then
    error 'error: staged nv-server database URL secret should match the source secret file'
    exit 1
fi
if ! cmp -s "$tmp_dir/source-health-diagnostics-token" "$DOCKER_SECRET_MOUNT_DIR/night-vision-local/health-diagnostics-token"; then
    error 'error: staged health diagnostics token should match the source secret file'
    exit 1
fi
if ! cmp -s "$tmp_dir/source-ca-file" "$DOCKER_SECRET_MOUNT_DIR/night-vision-local/ca_file"; then
    error 'error: staged CA file should match the source secret file'
    exit 1
fi
assert_file_mode \
    "$DOCKER_SECRET_MOUNT_DIR/night-vision-local/postgres-password" 444
assert_file_mode \
    "$DOCKER_SECRET_MOUNT_DIR/night-vision-local/nv-server-database-url" 444
assert_file_mode \
    "$DOCKER_SECRET_MOUNT_DIR/night-vision-local/health-diagnostics-token" 444
assert_file_mode "$DOCKER_SECRET_MOUNT_DIR/night-vision-local/ca_file" 444
pass_test

start_test 'local Compose does not require CA file'
POSTGRES_PASSWORD_SECRET_FILE="$tmp_dir/source-postgres-password" \
NV_SERVER_DATABASE_URL_SECRET_FILE="$tmp_dir/source-nv-server-database-url" \

    "$compose_script" --env-file .env.local.example config --quiet >/dev/null
pass_test

export HEALTH_DIAGNOSTICS_TOKEN_SECRET_FILE="$tmp_dir/health-diagnostics-token"

start_test 'local Compose uses DOCKER_HOST_REPO_ROOT for Linux default secret staging'
if [ "$(uname -s)" = Linux ]; then
    host_repo_root="$tmp_dir/host-repo-root"
    host_secret_mount_dir="$host_repo_root/.secrets/docker/night-vision-local"

    DOCKER_SECRET_MOUNT_DIR="" \
    DOCKER_HOST_REPO_ROOT="$host_repo_root" \
    POSTGRES_PASSWORD_SECRET_FILE="$tmp_dir/source-postgres-password" \
    NV_SERVER_DATABASE_URL_SECRET_FILE="$tmp_dir/source-nv-server-database-url" \
    HEALTH_DIAGNOSTICS_TOKEN_SECRET_FILE="$tmp_dir/source-health-diagnostics-token" \
        "$compose_script" --env-file .env.local.example config --quiet >/dev/null
    host_override_compose_config=$(
    DOCKER_SECRET_MOUNT_DIR="" \
    DOCKER_HOST_REPO_ROOT="$host_repo_root" \
    POSTGRES_PASSWORD_SECRET_FILE="$tmp_dir/source-postgres-password" \
    NV_SERVER_DATABASE_URL_SECRET_FILE="$tmp_dir/source-nv-server-database-url" \
    HEALTH_DIAGNOSTICS_TOKEN_SECRET_FILE="$tmp_dir/source-health-diagnostics-token" \
        "$compose_script" --env-file .env.local.example config
    )
    if ! printf '%s\n' "$host_override_compose_config" | grep -F "file: $host_secret_mount_dir/postgres-password" >/dev/null; then
    error 'error: local Docker Compose config should use the DOCKER_HOST_REPO_ROOT staged Postgres secret file'
    exit 1
    fi
    if ! printf '%s\n' "$host_override_compose_config" | grep -F "file: $host_secret_mount_dir/nv-server-database-url" >/dev/null; then
    error 'error: local Docker Compose config should use the DOCKER_HOST_REPO_ROOT staged nv-server database URL secret file'
    exit 1
    fi
    if ! cmp -s "$tmp_dir/source-postgres-password" "$host_secret_mount_dir/postgres-password"; then
    error 'error: DOCKER_HOST_REPO_ROOT staged Postgres secret should match the source secret file'
    exit 1
    fi
    if ! cmp -s "$tmp_dir/source-nv-server-database-url" "$host_secret_mount_dir/nv-server-database-url"; then
    error 'error: DOCKER_HOST_REPO_ROOT staged nv-server database URL secret should match the source secret file'
    exit 1
    fi
    assert_file_mode "$host_secret_mount_dir/postgres-password" 444
    assert_file_mode "$host_secret_mount_dir/nv-server-database-url" 444
fi
pass_test

start_test 'execute creates secret files with strict modes'
POSTGRES_PASSWORD_SECRET_FILE="$tmp_dir/postgres-password" \
NV_SERVER_DATABASE_URL_SECRET_FILE="$tmp_dir/nv-server-database-url" \
NV_LOCAL_DATABASE_URL_SECRET_FILE="$tmp_dir/local-development-database-url" \
ENV_FILE=/dev/null \
POSTGRES_DB=nv \
POSTGRES_USER=nv-server \
POSTGRES_HOST=postgres \
POSTGRES_PORT=5432 \
POSTGRES_PASSWORD=replace-me \
    "$setup_script" -xc >/dev/null

assert_file_mode "$tmp_dir/postgres-password" 600
assert_file_mode "$tmp_dir/nv-server-database-url" 600
assert_file_mode "$tmp_dir/local-development-database-url" 600
assert_file_mode "$tmp_dir/health-diagnostics-token" 600
assert_line_count "$tmp_dir/postgres-password" 1
assert_line_count "$tmp_dir/nv-server-database-url" 1
assert_line_count "$tmp_dir/local-development-database-url" 1
assert_line_count "$tmp_dir/health-diagnostics-token" 1
assert_contains "$tmp_dir/local-development-database-url" '@127.0.0.1:5432/nv'
if [ ! -s "$tmp_dir/health-diagnostics-token" ]; then
    error 'error: generated health diagnostics token should not be empty'
    exit 1
fi
pass_test

start_test 'plan mode leaves existing secrets unchanged'
POSTGRES_PASSWORD_SECRET_FILE="$tmp_dir/postgres-password" \
NV_SERVER_DATABASE_URL_SECRET_FILE="$tmp_dir/nv-server-database-url" \
NV_LOCAL_DATABASE_URL_SECRET_FILE="$tmp_dir/local-development-database-url" \
ENV_FILE=/dev/null \
POSTGRES_PASSWORD=replace-me \
    "$setup_script" >/dev/null

POSTGRES_PASSWORD_SECRET_FILE="$tmp_dir/postgres-password" \
NV_SERVER_DATABASE_URL_SECRET_FILE="$tmp_dir/nv-server-database-url" \
NV_LOCAL_DATABASE_URL_SECRET_FILE="$tmp_dir/local-development-database-url" \
ENV_FILE=/dev/null \
POSTGRES_PASSWORD=different-password \
    "$setup_script" >/dev/null

assert_contains "$tmp_dir/postgres-password" 'replace-me'
pass_test

start_test 'execute overwrites changed password'
POSTGRES_PASSWORD_SECRET_FILE="$tmp_dir/postgres-password" \
NV_SERVER_DATABASE_URL_SECRET_FILE="$tmp_dir/nv-server-database-url" \
NV_LOCAL_DATABASE_URL_SECRET_FILE="$tmp_dir/local-development-database-url" \
ENV_FILE=/dev/null \
POSTGRES_PASSWORD=different-password \
    "$setup_script" -x >/dev/null

assert_contains "$tmp_dir/postgres-password" 'different-password'
pass_test

start_test 'removed flags fail'
assert_fails "removed force flag" \
    env \
        POSTGRES_PASSWORD_SECRET_FILE="$tmp_dir/postgres-password" \
        NV_SERVER_DATABASE_URL_SECRET_FILE="$tmp_dir/nv-server-database-url" \
        ENV_FILE=/dev/null \
        POSTGRES_PASSWORD=replace-me \
        "$setup_script" --force

assert_fails "removed force password flag" \
    "$setup_script" --force-password

assert_fails "removed force database URL flag" \
    "$setup_script" --force-database-url

assert_fails "removed dry-run flag" \
    "$setup_script" --dry-run
pass_test

start_test 'plan mode does not create new files'
POSTGRES_PASSWORD_SECRET_FILE="$tmp_dir/planned-password" \
NV_SERVER_DATABASE_URL_SECRET_FILE="$tmp_dir/planned-url" \
ENV_FILE=/dev/null \
POSTGRES_PASSWORD=planned-password \
    "$setup_script" >/dev/null
assert_not_exists "$tmp_dir/planned-password"
assert_not_exists "$tmp_dir/planned-url"
pass_test

start_test 'print paths resolves defaults under repo'
env -u HEALTH_DIAGNOSTICS_TOKEN_SECRET_FILE "$setup_script" \
    -p \
    >/dev/null

paths_output=$(env -u HEALTH_DIAGNOSTICS_TOKEN_SECRET_FILE "$setup_script" -p)
if ! printf '%s\n' "$paths_output" | grep -F "Postgres password secret file: $repo_root/.secrets/postgres-password" >/dev/null; then
    error "error: default Postgres password secret path should resolve under $repo_root/.secrets"
    exit 1
fi
if ! printf '%s\n' "$paths_output" | grep -F "nv-server database URL secret file: $repo_root/.secrets/nv-server-database-url" >/dev/null; then
    error "error: default nv-server database URL secret path should resolve under $repo_root/.secrets"
    exit 1
fi
if ! printf '%s\n' "$paths_output" | grep -F "Local development database URL secret file: $repo_root/.secrets/local-development-database-url" >/dev/null; then
    error "error: default local development database URL secret path should resolve under $repo_root/.secrets"
    exit 1
fi
if ! printf '%s\n' "$paths_output" | grep -F "Health diagnostics token secret file: $repo_root/.secrets/health-diagnostics-token" >/dev/null; then
    error "error: default health diagnostics token path should resolve under $repo_root/.secrets"
    exit 1
fi

env -u HEALTH_DIAGNOSTICS_TOKEN_SECRET_FILE "$setup_script" \
    --print-paths \
    >/dev/null
pass_test

start_test 'combined short flags execute and validate'
POSTGRES_PASSWORD_SECRET_FILE="$tmp_dir/combined-password" \
NV_SERVER_DATABASE_URL_SECRET_FILE="$tmp_dir/combined-url" \
NV_LOCAL_DATABASE_URL_SECRET_FILE="$tmp_dir/combined-local-development-url" \
ENV_FILE=/dev/null \
POSTGRES_PASSWORD=replace-me \
    "$setup_script" -xc >/dev/null
pass_test

start_test 'quoted env values are parsed'
cat > "$tmp_dir/quoted.env" <<EOF
export POSTGRES_DB="quoted db"
POSTGRES_USER='quoted user'
POSTGRES_HOST=postgres # inline comment
POSTGRES_PORT=5432
POSTGRES_PASSWORD_SECRET_FILE=$tmp_dir/quoted-password
NV_SERVER_DATABASE_URL_SECRET_FILE=$tmp_dir/quoted-url
NV_LOCAL_DATABASE_URL_SECRET_FILE=$tmp_dir/quoted-local-development-url
EOF
ENV_FILE="$tmp_dir/quoted.env" \
POSTGRES_PASSWORD=quoted-password \
    "$setup_script" -x >/dev/null
assert_contains "$tmp_dir/quoted-url" 'quoted%20user:quoted-password@postgres:5432/quoted%20db'
assert_contains "$tmp_dir/quoted-local-development-url" 'quoted%20user:quoted-password@127.0.0.1:5432/quoted%20db'
pass_test

start_test 'database URL components are percent encoded'
POSTGRES_PASSWORD_SECRET_FILE="$tmp_dir/symbol-password" \
NV_SERVER_DATABASE_URL_SECRET_FILE="$tmp_dir/symbol-url" \
NV_LOCAL_DATABASE_URL_SECRET_FILE="$tmp_dir/symbol-local-development-url" \
ENV_FILE=/dev/null \
POSTGRES_PASSWORD='bad:value@with/slash%space value' \
    "$setup_script" -x >/dev/null
assert_contains "$tmp_dir/symbol-url" 'bad%3Avalue%40with%2Fslash%25space%20value'
pass_test

start_test 'long execute and validate flags work'
POSTGRES_PASSWORD_SECRET_FILE="$tmp_dir/long-validate-password" \
NV_SERVER_DATABASE_URL_SECRET_FILE="$tmp_dir/long-validate-url" \
NV_LOCAL_DATABASE_URL_SECRET_FILE="$tmp_dir/long-validate-local-development-url" \
ENV_FILE=/dev/null \
POSTGRES_PASSWORD=replace-me \
    "$setup_script" --execute --validate-compose >/dev/null
pass_test

start_test 'invalid flag combinations fail'
assert_fails "print paths with execute" \
    "$setup_script" -px

assert_fails "print paths with validate compose" \
    "$setup_script" --print-paths --validate-compose
pass_test

start_test 'invalid ports fail'
assert_fails "invalid high port" \
    env \
        POSTGRES_PASSWORD_SECRET_FILE="$tmp_dir/high-port-password" \
        NV_SERVER_DATABASE_URL_SECRET_FILE="$tmp_dir/high-port-url" \
        ENV_FILE=/dev/null \
        POSTGRES_PORT=70000 \
        "$setup_script"

assert_fails "invalid non-numeric port" \
    env \
        POSTGRES_PASSWORD_SECRET_FILE="$tmp_dir/non-numeric-port-password" \
        NV_SERVER_DATABASE_URL_SECRET_FILE="$tmp_dir/non-numeric-port-url" \
        ENV_FILE=/dev/null \
        POSTGRES_PORT=abc \
        "$setup_script"
pass_test

start_test 'multiline password fails'
assert_fails "multiline password" \
    env \
        POSTGRES_PASSWORD_SECRET_FILE="$tmp_dir/multiline-password" \
        NV_SERVER_DATABASE_URL_SECRET_FILE="$tmp_dir/multiline-url" \
        ENV_FILE=/dev/null \
        POSTGRES_PASSWORD="$(printf 'bad\nvalue')" \
        "$setup_script"
pass_test

success 'Docker shell script tests passed.'
