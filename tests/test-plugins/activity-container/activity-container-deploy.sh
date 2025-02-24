#!/bin/bash

# Default values
IMAGE_TAR="./tests/test-plugins/activity-container/activity-image.tar"
IMAGE_NAME="activity-image"
PORT=8888

while [[ $# -gt 0 ]]; do
    if [[ "$1" == "--port" && -n "$2" && "$2" =~ ^[0-9]+$ ]]; then
        PORT="$2"
        shift 2
    else
        # Collect any other arguments to pass to docker run
        EXTRA_ARGS="$EXTRA_ARGS $1"
        shift
    fi
done

if [[ ! -f "$IMAGE_TAR" ]]; then
    echo "Error: Image tar file '$IMAGE_TAR' not found!"
    exit 1
fi


# Check if the image is already loaded
if ! docker images | grep -q "$IMAGE_NAME"; then
    echo "Image '$IMAGE_NAME' not found. Loading the image..."
    if ! docker load -i "$IMAGE_TAR" > /dev/null 2>&1; then
        echo "Error: Failed to load image '$IMAGE_TAR'."
        exit 1
    fi
fi
# Otherwise, the image is already loaded

# Format the run statement for container port mapping
docker run --init -p "$PORT":50051 activity-image
