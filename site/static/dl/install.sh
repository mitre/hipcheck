#!/usr/bin/env sh

# This installer delegates to the "real" installer included with each new
# release of Hipcheck.

HC_VERSION="3.7.0"
REPO="https://github.com/mitre/hipcheck"
INSTALLER="$REPO/releases/download/hipcheck-v$HC_VERSION/hipcheck-installer.sh"

# Check that curl is installed and error out if it isn't.
if ! command -v curl >/dev/null; then
    echo "error: 'curl' is required to run the installer" 1>&2
    exit 1
fi

curl -LsSf "$INSTALLER" | sh "$@"
