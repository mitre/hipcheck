#!/bin/sh

set -eu

bundle_root=/opt/night-vision/hipcheck
cache_root=/var/cache/night-vision/hipcheck

mkdir -p "$cache_root/plugins/binary" "$cache_root/target/release"
cp "$bundle_root/plugins/binary/local-release-plugin.kdl" \
    "$cache_root/plugins/binary/local-release-plugin.kdl"
cp "$bundle_root/target/release/binary" "$cache_root/target/release/binary"

exec /usr/local/bin/nv-server "$@"
