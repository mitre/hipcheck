#!/bin/sh

# Create local Docker Compose secret files for Night Vision.
#
# Usage:
#
#   docker/scripts/setup-compose-secrets.sh
#   docker/scripts/setup-compose-secrets.sh -x
#   docker/scripts/setup-compose-secrets.sh -p
#   POSTGRES_PASSWORD='replace-me' docker/scripts/setup-compose-secrets.sh -x
#
# The script reads non-secret database settings from `.env` by default and
# plans the matching Postgres password and `nv-server` database URL secret
# files. Use `-x` or `--execute` to write the files. Use `ENV_FILE`,
# `POSTGRES_PASSWORD_SECRET_FILE`, or
# `NV_SERVER_DATABASE_URL_SECRET_FILE` to override the defaults.

set -eu
umask 077

if [ -t 1 ] && [ -z "${NO_COLOR:-}" ]; then
    color_green=$(printf '\033[32m')
    color_yellow=$(printf '\033[33m')
    color_blue=$(printf '\033[36m')
    color_red=$(printf '\033[31m')
    color_bold=$(printf '\033[1m')
    color_reset=$(printf '\033[0m')
else
    color_green=
    color_yellow=
    color_blue=
    color_red=
    color_bold=
    color_reset=
fi

section() {
    printf '%s%s%s\n' "$color_bold$color_blue" "$1" "$color_reset"
}

info() {
    printf '%s%s%s\n' "$color_blue" "$1" "$color_reset"
}

success() {
    printf '%s%s%s\n' "$color_green" "$1" "$color_reset"
}

plan() {
    printf '%s%s%s\n' "$color_yellow" "$1" "$color_reset"
}

error() {
    printf '%s%s%s\n' "$color_red" "$1" "$color_reset" >&2
}

usage() {
    section 'Usage:'
    printf '  %sdocker/scripts/setup-compose-secrets.sh [-c]%s\n' "$color_yellow" "$color_reset"
    printf '  %sdocker/scripts/setup-compose-secrets.sh -x [-c]%s\n' "$color_yellow" "$color_reset"
    printf '  %sdocker/scripts/setup-compose-secrets.sh -p%s\n' "$color_yellow" "$color_reset"
    printf '  %sPOSTGRES_PASSWORD='\''replace-me'\'' docker/scripts/setup-compose-secrets.sh -x%s\n\n' "$color_yellow" "$color_reset"

    cat <<'EOF'
Plans local Docker Compose secret files for Postgres and nv-server. Pass
-x/--execute to write the planned changes. By default, non-secret database
settings are read from .env in the repository root.
The .env parser supports KEY=value and export KEY=value lines, quoted values,
and whitespace-prefixed inline comments on unquoted values.

EOF
    section 'Options:'
    cat <<'EOF'
  -x, --execute           Write the planned secret file changes.
  -p, --print-paths       Print resolved env and secret file paths, then exit.
  -c, --validate-compose  Validate the local Docker Compose config.
  -h, --help              Show this help.

EOF
    section 'Environment overrides:'
    cat <<'EOF'
  ENV_FILE
      Path to the non-secret env file. Defaults to .env in the repository root.
  POSTGRES_DB
      Database name used in the generated nv-server database URL.
  POSTGRES_USER
      Database user used by Postgres and in the generated database URL.
  POSTGRES_HOST
      Database hostname used in the generated nv-server database URL.
  POSTGRES_PORT
      Database TCP port used in the generated nv-server database URL.
  POSTGRES_PASSWORD
      Password to write to the Postgres password secret file.
  POSTGRES_PASSWORD_SECRET_FILE
      Path to the Postgres password secret file.
  NV_SERVER_DATABASE_URL_SECRET_FILE
      Path to the nv-server database URL secret file.

POSTGRES_USER, POSTGRES_PASSWORD, and POSTGRES_DB are percent-encoded before the
database URL is written.
EOF
}

execute=false
print_paths=false
validate_compose=false

while [ "$#" -gt 0 ]; do
    case "$1" in
        -x|--execute)
            execute=true
            ;;
        -p|--print-paths)
            print_paths=true
            ;;
        -c|--validate-compose)
            validate_compose=true
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        -[!-]?*)
            short_flags=$(printf '%s\n' "$1" | sed 's/^-//')
            while [ -n "$short_flags" ]; do
                flag=${short_flags%"${short_flags#?}"}
                short_flags=${short_flags#?}
                case "$flag" in
                    c)
                        validate_compose=true
                        ;;
                    h)
                        usage
                        exit 0
                        ;;
                    p)
                        print_paths=true
                        ;;
                    x)
                        execute=true
                        ;;
                    *)
                        usage >&2
                        exit 2
                        ;;
                esac
            done
            ;;
        *)
            usage >&2
            exit 2
            ;;
    esac
    shift
done

if [ "$print_paths" = true ] && {
    [ "$execute" = true ] \
        || [ "$validate_compose" = true ]
}; then
    error 'error: -p/--print-paths cannot be combined with -x/--execute or -c/--validate-compose'
    exit 2
fi

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
env_file=${ENV_FILE:-"$repo_root/.env"}

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

require_python3() {
    if ! command -v python3 >/dev/null 2>&1; then
        error 'error: python3 is required to parse .env files and percent-encode database URLs'
        exit 1
    fi
}

env_value() {
    key=$1
    default=$2

    current=$(printenv "$key" 2>/dev/null || true)
    if [ -n "$current" ]; then
        printf '%s\n' "$current"
        return
    fi

    if [ -f "$env_file" ]; then
        from_file=$(
            python3 - "$env_file" "$key" <<'PY'
import re
import shlex
import sys

env_file, requested_key = sys.argv[1], sys.argv[2]
value = None

with open(env_file, encoding="utf-8") as handle:
    for raw_line in handle:
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        if line.startswith("export "):
            line = line[7:].lstrip()
        if "=" not in line:
            continue
        key, raw_value = line.split("=", 1)
        if key.strip() != requested_key:
            continue

        raw_value = raw_value.strip()
        if raw_value.startswith(("'", '"')):
            try:
                parts = shlex.split(raw_value, comments=False, posix=True)
            except ValueError as error:
                print(f"error: failed to parse {requested_key} in {env_file}: {error}", file=sys.stderr)
                sys.exit(2)
            value = parts[0] if parts else ""
        else:
            value = re.sub(r"\s+#.*$", "", raw_value).strip()

if value is not None:
    print(value, end="")
PY
        )
        if [ -n "$from_file" ]; then
            printf '%s\n' "$from_file"
            return
        fi
    fi

    printf '%s\n' "$default"
}

require_nonempty() {
    name=$1
    value=$2

    if [ -z "$value" ]; then
        error "error: $name must not be empty"
        exit 1
    fi
}

require_single_line() {
    name=$1
    value=$2

    case "$value" in
        *'
'*)
            error "error: $name must not contain newlines"
            exit 1
            ;;
    esac
}

percent_encode() {
    name=$1
    value=$2

    require_single_line "$name" "$value"
    printf '%s' "$value" | python3 -c 'import sys, urllib.parse; print(urllib.parse.quote(sys.stdin.read(), safe="-._~"), end="")'
}

require_port() {
    name=$1
    value=$2

    case "$value" in
        *[!0123456789]*)
            error "error: $name must be a numeric TCP port"
            exit 1
            ;;
    esac

    if [ "$value" -lt 1 ] || [ "$value" -gt 65535 ]; then
        error "error: $name must be between 1 and 65535"
        exit 1
    fi
}

file_content() {
    path=$1

    if [ -f "$path" ]; then
        cat "$path"
    fi
}

plan_file_action() {
    path=$1
    content=$2

    if [ ! -e "$path" ]; then
        printf 'create\n'
        return
    fi

    if [ "$(file_content "$path")" = "$content" ]; then
        printf 'leave unchanged\n'
        return
    fi

    printf 'overwrite\n'
}

write_secret_file() {
    label=$1
    path=$2
    content=$3
    action=$4

    case "$action" in
        "create"|"overwrite")
            install -d -m 700 "$(dirname "$path")"
            printf '%s\n' "$content" > "$path"
            chmod 600 "$path"
            success "$action $label: $path"
            ;;
        "leave unchanged")
            info "Left $label unchanged: $path"
            ;;
        *)
            error "error: unknown file action for $label: $action"
            exit 1
            ;;
    esac
}

require_python3

postgres_db=$(env_value POSTGRES_DB nv)
postgres_user=$(env_value POSTGRES_USER nv-server)
postgres_host=$(env_value POSTGRES_HOST postgres)
postgres_port=$(env_value POSTGRES_PORT 5432)
postgres_password_secret_file=$(absolute_path "$(env_value POSTGRES_PASSWORD_SECRET_FILE .secrets/postgres-password)")
nv_server_database_url_secret_file=$(absolute_path "$(env_value NV_SERVER_DATABASE_URL_SECRET_FILE .secrets/nv-server-database-url)")

require_nonempty POSTGRES_DB "$postgres_db"
require_nonempty POSTGRES_USER "$postgres_user"
require_nonempty POSTGRES_HOST "$postgres_host"
require_nonempty POSTGRES_PORT "$postgres_port"
require_nonempty POSTGRES_PASSWORD_SECRET_FILE "$postgres_password_secret_file"
require_nonempty NV_SERVER_DATABASE_URL_SECRET_FILE "$nv_server_database_url_secret_file"
require_single_line POSTGRES_DB "$postgres_db"
require_single_line POSTGRES_USER "$postgres_user"
require_single_line POSTGRES_HOST "$postgres_host"
require_single_line POSTGRES_PORT "$postgres_port"
require_port POSTGRES_PORT "$postgres_port"

if [ "$print_paths" = true ]; then
    info "Env file: $env_file"
    info "Postgres password secret file: $postgres_password_secret_file"
    info "nv-server database URL secret file: $nv_server_database_url_secret_file"
    exit 0
fi

postgres_password=${POSTGRES_PASSWORD:-}
postgres_password_source="POSTGRES_PASSWORD environment variable"
if [ -z "$postgres_password" ] && [ -f "$postgres_password_secret_file" ]; then
    postgres_password=$(cat "$postgres_password_secret_file")
    postgres_password_source="existing Postgres password secret"
fi
if [ -z "$postgres_password" ]; then
    postgres_password=change-me
    postgres_password_source="local default"
fi

require_nonempty POSTGRES_PASSWORD "$postgres_password"
require_single_line POSTGRES_PASSWORD "$postgres_password"

encoded_postgres_user=$(percent_encode POSTGRES_USER "$postgres_user")
encoded_postgres_password=$(percent_encode POSTGRES_PASSWORD "$postgres_password")
encoded_postgres_db=$(percent_encode POSTGRES_DB "$postgres_db")

database_url=$(printf 'postgres://%s:%s@%s:%s/%s' \
    "$encoded_postgres_user" \
    "$encoded_postgres_password" \
    "$postgres_host" \
    "$postgres_port" \
    "$encoded_postgres_db")

password_action=$(plan_file_action "$postgres_password_secret_file" "$postgres_password")
database_url_action=$(plan_file_action "$nv_server_database_url_secret_file" "$database_url")

info "Using Postgres password source: $postgres_password_source"

if [ "$execute" = false ]; then
    plan "Would $password_action Postgres password secret: $postgres_password_secret_file"
    plan "Would $database_url_action nv-server database URL secret: $nv_server_database_url_secret_file"
    info 'Pass -x or --execute to apply these changes.'
else
    write_secret_file "Postgres password secret" "$postgres_password_secret_file" "$postgres_password" "$password_action"
    write_secret_file "nv-server database URL secret" "$nv_server_database_url_secret_file" "$database_url" "$database_url_action"
fi

if [ "$validate_compose" = true ]; then
    ENV_FILE="$env_file" \
    POSTGRES_DB="$postgres_db" \
    POSTGRES_USER="$postgres_user" \
    POSTGRES_HOST="$postgres_host" \
    POSTGRES_PORT="$postgres_port" \
    POSTGRES_PASSWORD_SECRET_FILE="$postgres_password_secret_file" \
    NV_SERVER_DATABASE_URL_SECRET_FILE="$nv_server_database_url_secret_file" \
        "$script_dir/docker-compose-local.sh" --env-file "$env_file" config --quiet
    success 'Validated local Docker Compose config.'
fi
