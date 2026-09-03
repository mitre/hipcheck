#!/bin/sh

# Run Docker Compose with Night Vision's local-development override.
#
# Usage:
#
#   scripts/docker-compose-local.sh up --build
#   scripts/docker-compose-local.sh down -v
#
# The wrapper applies `docker-compose.local.yml`, stages local secret files into
# a Docker-approved host mount directory, sets a local Compose project name, and
# defaults the local service image names. If `CA_FILE_SECRET_FILE` is set, it is
# staged and passed to the backend Dockerfile as the `ca_file` build secret. Set
# `DOCKER_HOST_REPO_ROOT` when the Docker daemon needs a host-visible path for
# the repository root that differs from the current shell's path (for example,
# docker-out-of-docker from a devcontainer on a Windows host). Pass any normal
# `docker compose` arguments after the script name.

set -eu

if [ -t 1 ] && [ -z "${NO_COLOR:-}" ]; then
    color_blue=$(printf '\033[36m')
    color_yellow=$(printf '\033[33m')
    color_bold=$(printf '\033[1m')
    color_reset=$(printf '\033[0m')
else
    color_blue=
    color_yellow=
    color_bold=
    color_reset=
fi

section() {
    printf '%s%s%s\n' "$color_bold$color_blue" "$1" "$color_reset"
}

info() {
    printf '%s%s%s\n' "$color_blue" "$1" "$color_reset"
}

error() {
    printf '%s\n' "$1" >&2
}

usage() {
    section 'Usage:'
    printf '  %sscripts/docker-compose-local.sh [docker compose args...]%s\n\n' "$color_yellow" "$color_reset"
    section 'Examples:'
    printf '  %sscripts/docker-compose-local.sh up --build%s\n' "$color_yellow" "$color_reset"
    printf '  %sscripts/docker-compose-local.sh down -v%s\n\n' "$color_yellow" "$color_reset"
    cat <<'EOF'
Runs Docker Compose from the repository root with docker-compose.yml and
docker-compose.local.yml. COMPOSE_PROJECT_NAME defaults to night-vision-local,
NV_APP_IMAGE defaults to nv-app:local, NV_SERVER_IMAGE defaults to
nv-server:local, DOCKER_SECRET_MOUNT_DIR overrides the Docker-approved host
directory used for staged local secrets, and DOCKER_HOST_REPO_ROOT overrides the
repository root used to derive the default Linux staging directory when Docker
needs a host-visible path. Set CA_FILE_SECRET_FILE to pass network CA
certificates to the backend image build. Export HIPCHECK_GITLAB_TOKEN for
commands that build the backend image; Compose passes it only as a BuildKit
secret.
EOF
}

case "${1:-}" in
    -h|--help)
        usage
        exit 0
        ;;
esac

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

absolute_path() {
    path=$1
    case "$path" in
        /*)
            printf '%s\n' "$path"
            ;;
        *)
            printf '%s\n' "$repo_root/$path"
            ;;
    esac
}

env_file_value() {
    key=$1
    default=$2

    current=$(printenv "$key" 2>/dev/null || true)
    if [ -n "$current" ]; then
        printf '%s\n' "$current"
        return
    fi

    if [ -f "$repo_root/.env" ]; then
        value=$(
            awk -F= -v key="$key" '
                $1 ~ "^[[:space:]]*" key "[[:space:]]*$" {
                    sub(/^[[:space:]]*/, "", $2)
                    sub(/[[:space:]]*#.*$/, "", $2)
                    value = $2
                }
                END {
                    if (value != "") {
                        print value
                    }
                }
            ' "$repo_root/.env"
        )
        if [ -n "$value" ]; then
            printf '%s\n' "$value"
            return
        fi
    fi

    printf '%s\n' "$default"
}

host_repo_root() {
    if [ -n "${DOCKER_HOST_REPO_ROOT:-}" ]; then
        printf '%s\n' "$DOCKER_HOST_REPO_ROOT"
    else
        printf '%s\n' "$repo_root"
    fi
}

approved_mount_root() {
    if [ -n "${DOCKER_SECRET_MOUNT_DIR:-}" ]; then
        printf '%s\n' "$DOCKER_SECRET_MOUNT_DIR"
        return
    fi

    os=$(uname -s)
    case "$os" in
        Darwin)
            printf '%s\n' /Users/Shared/Docker/night-vision
            ;;
        CYGWIN*|MINGW*|MSYS*)
            printf '%s\n' /c/Users/Public/Docker/night-vision
            ;;
        Linux)
            if grep -qi microsoft /proc/version 2>/dev/null; then
                printf '%s\n' /mnt/c/Users/Public/Docker/night-vision
            else
                printf '%s\n' "$(host_repo_root)/.secrets/docker"
            fi
            ;;
        *)
            error "error: unsupported platform for Docker-approved secret staging: $os"
            error 'Set DOCKER_SECRET_MOUNT_DIR to a Docker-approved host mount path.'
            exit 1
            ;;
    esac
}

stage_secret_file() {
    label=$1
    source_path=$2
    target_path=$3

    if [ ! -f "$source_path" ]; then
        error "error: missing $label: $source_path"
        error 'Run scripts/setup-compose-secrets.sh -x first.'
        exit 1
    fi

    install -d -m 700 "$(dirname "$target_path")"
    install -m 444 "$source_path" "$target_path"
}

yaml_single_quote() {
    printf "'"
    printf '%s' "$1" | sed "s/'/''/g"
    printf "'\n"
}

write_ca_file_compose_override() {
    target_path=$1
    ca_file_secret_file=$2
    quoted_ca_file_secret_file=$(yaml_single_quote "$ca_file_secret_file")

    install -d -m 700 "$(dirname "$target_path")"
    {
        printf '%s\n' 'services:'
        printf '%s\n' '  nv-server:'
        printf '%s\n' '    build:'
        printf '%s\n' '      secrets:'
        printf '%s\n' '        - source: ca_file'
        printf '%s\n' '          target: ca_file'
        printf '%s\n' ''
        printf '%s\n' 'secrets:'
        printf '%s\n' '  ca_file:'
        printf '    file: %s\n' "$quoted_ca_file_secret_file"
    } > "$target_path"
    chmod 600 "$target_path"
}

source_postgres_password_secret_file=$(absolute_path "$(env_file_value POSTGRES_PASSWORD_SECRET_FILE .secrets/postgres-password)")
source_nv_server_database_url_secret_file=$(absolute_path "$(env_file_value NV_SERVER_DATABASE_URL_SECRET_FILE .secrets/nv-server-database-url)")
source_ca_file_secret_file=$(env_file_value CA_FILE_SECRET_FILE "")

export COMPOSE_PROJECT_NAME="${COMPOSE_PROJECT_NAME:-night-vision-local}"
export NV_APP_IMAGE="${NV_APP_IMAGE:-nv-app:local}"
export NV_SERVER_IMAGE="${NV_SERVER_IMAGE:-nv-server:local}"

staged_secret_dir=$(approved_mount_root)/$COMPOSE_PROJECT_NAME
postgres_password_secret_file=$staged_secret_dir/postgres-password
nv_server_database_url_secret_file=$staged_secret_dir/nv-server-database-url
ca_file_compose_override_file=

stage_secret_file "Postgres password secret" "$source_postgres_password_secret_file" "$postgres_password_secret_file"
stage_secret_file "nv-server database URL secret" "$source_nv_server_database_url_secret_file" "$nv_server_database_url_secret_file"

export POSTGRES_PASSWORD_SECRET_FILE="$postgres_password_secret_file"
export NV_SERVER_DATABASE_URL_SECRET_FILE="$nv_server_database_url_secret_file"

if [ -n "$source_ca_file_secret_file" ]; then
    source_ca_file_secret_file=$(absolute_path "$source_ca_file_secret_file")
    ca_file_secret_file=$staged_secret_dir/ca_file
    ca_file_compose_override_file=$staged_secret_dir/docker-compose.ca-file.yml

    stage_secret_file "CA file secret" "$source_ca_file_secret_file" "$ca_file_secret_file"
    write_ca_file_compose_override "$ca_file_compose_override_file" "$ca_file_secret_file"
fi

cd "$repo_root"

info "Running local Docker Compose project: $COMPOSE_PROJECT_NAME"

if [ -n "$ca_file_compose_override_file" ]; then
    exec docker compose \
        -f docker-compose.yml \
        -f docker-compose.local.yml \
        -f "$ca_file_compose_override_file" \
        "$@"
else
    exec docker compose \
        -f docker-compose.yml \
        -f docker-compose.local.yml \
        "$@"
fi
