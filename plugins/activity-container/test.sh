#!/bin/bash

# Default values
IMAGE_TAR="./plugins/activity-container/activity-image.tar"
PORT=8888

while [[ $# -gt 0 ]]; do
    if [[ "$1" == "--port" && -n "$2" && "$2" =~ ^[0-9]+$ ]]; then
        PORT="$2"
        shift 2
    else
        echo "Unknown or invalid argument: $1"
        exit 1
    fi
done

if [[ ! -f "$IMAGE_TAR" ]]; then
    echo "Error: Image tar file '$IMAGE_TAR' not found!"
    exit 1
fi


# Import the tar file and run the Docker image
docker load -i "$IMAGE_TAR"
# echo "$PORT"

# docker run --init -it activity-image /bin/bash
docker run --init -p "$PORT":50051 activity-image
# docker run -it --init  -e HIPCHECK_PORT="$PORT" -p 8888:8080 activity-image
