#!/bin/sh

# Run the CI Compose smoke test with Docker Desktop secret staging.

set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_root=$(CDPATH= cd -- "$script_dir/.." && pwd)

if [ -z "${DOCKER_COMPOSE_TMPDIR:-}" ]; then
    case "$(uname -s)" in
        Darwin)
            DOCKER_COMPOSE_TMPDIR=/Users/Shared/Docker/night-vision/compose-smoke
            ;;
        CYGWIN*|MINGW*|MSYS*)
            DOCKER_COMPOSE_TMPDIR=C:/Users/Public/Docker/night-vision/compose-smoke
            ;;
        *)
            DOCKER_COMPOSE_TMPDIR=$repo_root/compose-smoke-secrets
            ;;
    esac
fi

mkdir -p "$DOCKER_COMPOSE_TMPDIR"
export DOCKER_COMPOSE_TMPDIR
export CI_JOB_ID="${CI_JOB_ID:-local-$$}"

exec "$script_dir/test-compose-smoke.sh"
