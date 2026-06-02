#!/bin/sh

# Run smoke tests for Night Vision's Docker shell scripts.
#
# Usage:
#
#   docker/scripts/test.sh
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

script_dir=$(
    CDPATH=
    cd -- "$(dirname -- "$0")"
    pwd
)
repo_root=$(
    CDPATH=
    cd -- "$script_dir/../.."
    pwd
)

tmp_dir=$(mktemp -d "${TMPDIR:-/tmp}/night-vision-docker-scripts.XXXXXX")
trap 'rm -rf "$tmp_dir"' EXIT HUP INT TERM

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

    mode=$(stat -f '%Lp' "$path" 2>/dev/null || stat -c '%a' "$path")
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
shellcheck "$script_dir"/*.sh
sh -n "$script_dir"/*.sh
"$compose_script" --env-file .env.local.example config --quiet

POSTGRES_PASSWORD_SECRET_FILE="$tmp_dir/postgres-password" \
NV_SERVER_DATABASE_URL_SECRET_FILE="$tmp_dir/nv-server-database-url" \
ENV_FILE=/dev/null \
POSTGRES_DB=nv \
POSTGRES_USER=nv-server \
POSTGRES_HOST=postgres \
POSTGRES_PORT=5432 \
POSTGRES_PASSWORD=replace-me \
    "$setup_script" -xc >/dev/null

assert_file_mode "$tmp_dir/postgres-password" 600
assert_file_mode "$tmp_dir/nv-server-database-url" 600
assert_line_count "$tmp_dir/postgres-password" 1
assert_line_count "$tmp_dir/nv-server-database-url" 1

POSTGRES_PASSWORD_SECRET_FILE="$tmp_dir/postgres-password" \
NV_SERVER_DATABASE_URL_SECRET_FILE="$tmp_dir/nv-server-database-url" \
ENV_FILE=/dev/null \
POSTGRES_PASSWORD=replace-me \
    "$setup_script" >/dev/null

POSTGRES_PASSWORD_SECRET_FILE="$tmp_dir/postgres-password" \
NV_SERVER_DATABASE_URL_SECRET_FILE="$tmp_dir/nv-server-database-url" \
ENV_FILE=/dev/null \
POSTGRES_PASSWORD=different-password \
    "$setup_script" >/dev/null

assert_contains "$tmp_dir/postgres-password" 'replace-me'

POSTGRES_PASSWORD_SECRET_FILE="$tmp_dir/postgres-password" \
NV_SERVER_DATABASE_URL_SECRET_FILE="$tmp_dir/nv-server-database-url" \
ENV_FILE=/dev/null \
POSTGRES_PASSWORD=different-password \
    "$setup_script" -x >/dev/null

assert_contains "$tmp_dir/postgres-password" 'different-password'

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

POSTGRES_PASSWORD_SECRET_FILE="$tmp_dir/planned-password" \
NV_SERVER_DATABASE_URL_SECRET_FILE="$tmp_dir/planned-url" \
ENV_FILE=/dev/null \
POSTGRES_PASSWORD=planned-password \
    "$setup_script" >/dev/null
assert_not_exists "$tmp_dir/planned-password"
assert_not_exists "$tmp_dir/planned-url"

"$setup_script" \
    -p \
    >/dev/null

"$setup_script" \
    --print-paths \
    >/dev/null

POSTGRES_PASSWORD_SECRET_FILE="$tmp_dir/combined-password" \
NV_SERVER_DATABASE_URL_SECRET_FILE="$tmp_dir/combined-url" \
ENV_FILE=/dev/null \
POSTGRES_PASSWORD=replace-me \
    "$setup_script" -xc >/dev/null

POSTGRES_PASSWORD_SECRET_FILE="$tmp_dir/symbol-password" \
NV_SERVER_DATABASE_URL_SECRET_FILE="$tmp_dir/symbol-url" \
ENV_FILE=/dev/null \
POSTGRES_PASSWORD='bad:value@with/slash%space value' \
    "$setup_script" -x >/dev/null
assert_contains "$tmp_dir/symbol-url" 'bad%3Avalue%40with%2Fslash%25space%20value'

POSTGRES_PASSWORD_SECRET_FILE="$tmp_dir/long-validate-password" \
NV_SERVER_DATABASE_URL_SECRET_FILE="$tmp_dir/long-validate-url" \
ENV_FILE=/dev/null \
POSTGRES_PASSWORD=replace-me \
    "$setup_script" --execute --validate-compose >/dev/null

assert_fails "print paths with execute" \
    "$setup_script" -px

assert_fails "print paths with validate compose" \
    "$setup_script" --print-paths --validate-compose

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

assert_fails "multiline password" \
    env \
        POSTGRES_PASSWORD_SECRET_FILE="$tmp_dir/multiline-password" \
        NV_SERVER_DATABASE_URL_SECRET_FILE="$tmp_dir/multiline-url" \
        ENV_FILE=/dev/null \
        POSTGRES_PASSWORD="$(printf 'bad\nvalue')" \
        "$setup_script"

success 'Docker shell script tests passed.'
