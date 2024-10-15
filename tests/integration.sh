#!/usr/bin/env sh

# SPDX-License-Identifier: Apache 2.0

# Some of the following is adapted from 'rustup-init.sh' from the Rust project
# under the terms of the Apache 2.0 license.
#
# https://github.com/rust-lang/rustup/blob/3db381b0bec0f8f36351d431aae723654e4261ae/rustup-init.sh

__print() {
    printf '%s: %s\n' "$1" "$2" >&2
}

warn() {
    __print 'warn' "$1" >&2
}

# NOTE: you are required to exit yourself
# we don't do it here because of multiline errors
err() {
    __print 'error' "$1" >&2
}

need_cmd() {
    if ! check_cmd "$1"; then
        err "need '$1' (command not found)"
        exit 1
    fi
}

check_cmd() {
    command -v "$1" > /dev/null 2>&1
}

# Run a command that should never fail. If the command fails execution
# will immediately terminate with an error showing the failing
# command.
ensure() {
    if ! "$@"; then
        err "command failed: $*"
        exit 1
    fi
}

hipcheck_is_ready() {
    cargo run --quiet -p hipcheck -- ready | grep "Hipcheck is ready to run"
}

setup() {
    # Build all the crates we need.
    cargo build --quiet -p hipcheck -p dummy_rand_data_sdk -p dummy_sha256_sdk 1>/dev/null 2>&1

    # Setup Hipcheck if it's not ready.
    if ! hipcheck_is_ready; then
        cargo run --quiet -p hipcheck -- setup
    fi
}

can_run() {
    warn "running 'can_run'"

    ref="fddb21f"
    pkg="pkg:github/mitre/hipcheck"

    cargo run --quiet -p hipcheck -- check -v quiet --ref "$ref" "$pkg" 1>/dev/null
}

can_run_with_plugins() {
    warn "running 'can_run_with_plugins'"
    HC_LOG=debug cargo run --quiet -p hipcheck -- plugin 1>/dev/null
}

main() {
    setup
    ensure can_run
    ensure can_run_with_plugins
}

main
