#!/bin/bash

SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )

# Go to Python SDK location
pushd $SCRIPT_DIR

# Install setuptools
python3 -m pip install --upgrade build

# Build as a wheel
python3 -m build --wheel

# Return to caller location
popd
