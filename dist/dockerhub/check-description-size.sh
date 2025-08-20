#!/usr/bin/env bash

set -eu

FILE_PATH="./dist/dockerhub/README.md"
MAX_SIZE=25000

if [ ! -f "$FILE_PATH" ]; then
    echo "File does not exist: $FILE_PATH"
    exit 1
fi

FILE_SIZE=$(wc -c < $FILE_PATH)

if [ "$FILE_SIZE" -ge $MAX_SIZE ]; then
    echo "File is too large: $FILE_SIZE bytes (MAX allowed is $MAX_SIZE)"
    exit 1
fi

echo "File is small enough to push: $FILE_SIZE (MAX allowed is $MAX_SIZE)"
