#!/bin/sh

# Run Docker Compose with Night Vision's local-development override.
#
# Usage:
#
#   docker/scripts/docker-compose-local.sh up --build
#   docker/scripts/docker-compose-local.sh down -v
#
# The wrapper applies `docker-compose.local.yml`, sets a local Compose project
# name, and defaults `NV_SERVER_IMAGE` to `nv-server:local`. Pass any normal
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

usage() {
    section 'Usage:'
    printf '  %sdocker/scripts/docker-compose-local.sh [docker compose args...]%s\n\n' "$color_yellow" "$color_reset"
    section 'Examples:'
    printf '  %sdocker/scripts/docker-compose-local.sh up --build%s\n' "$color_yellow" "$color_reset"
    printf '  %sdocker/scripts/docker-compose-local.sh down -v%s\n\n' "$color_yellow" "$color_reset"
    cat <<'EOF'
Runs Docker Compose from the repository root with docker-compose.yml and
docker-compose.local.yml. COMPOSE_PROJECT_NAME defaults to night-vision-local,
and NV_SERVER_IMAGE defaults to nv-server:local.
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
    cd -- "$script_dir/../.."
    pwd
)

export COMPOSE_PROJECT_NAME="${COMPOSE_PROJECT_NAME:-night-vision-local}"
export NV_SERVER_IMAGE="${NV_SERVER_IMAGE:-nv-server:local}"

cd "$repo_root"

info "Running local Docker Compose project: $COMPOSE_PROJECT_NAME"

exec docker compose \
    -f docker-compose.yml \
    -f docker-compose.local.yml \
    "$@"
