# syntax=docker/dockerfile:1

# Build with:
#
#   docker build \
#     --secret id=ca_file,src="$HOME/.config/certs/SystemCerts.pem" \
#     --build-arg IMAGE_CREATED="$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
#     --build-arg IMAGE_REVISION="$(git rev-parse HEAD)" \
#     --build-arg IMAGE_VERSION="0.1.0" \
#     -f backend/Dockerfile \
#     -t nv-server:local \
#     backend

# Keep these versions synced with the versions set in `.flox/env/manifest.toml`.
ARG RUST_VERSION=1.95.0-bookworm
ARG CARGO_CHEF_VERSION=0.1.77

# OCI Image Metadata.
ARG IMAGE_CREATED
ARG IMAGE_REVISION
ARG IMAGE_SOURCE=https://github.com/mitre/hipcheck/night-vision
ARG IMAGE_VERSION=0.1.0

# ================================================================================================
# Chef Layer
# ------------------------------------------------------------------------------------------------

FROM rust:${RUST_VERSION} AS chef

ARG RUST_VERSION
ARG CARGO_CHEF_VERSION

WORKDIR /usr/src/night-vision

# Create an optional secret mount to pass any necessary corporate network certs into the container image.
#
# Use `--secret id=ca_file,src=<CA_FILE>` to pass the CA cert file into the container.
#
# See: https://docs.docker.com/build/building/secrets/#secret-mounts
RUN --mount=type=secret,id=ca_file,target=/usr/local/share/ca-certificates/build-ca.crt,required=false \
    # Use cache mounts to improve rebuild performance.
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    update-ca-certificates \
    && cargo install cargo-chef --locked --version "${CARGO_CHEF_VERSION}"

# ================================================================================================
# Planner Layer
# ------------------------------------------------------------------------------------------------

FROM chef AS planner

COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ================================================================================================
# Builder Layer
# ------------------------------------------------------------------------------------------------

FROM chef AS builder

COPY --from=planner /usr/src/night-vision/recipe.json recipe.json

# Build dependencies in a separate layer so Docker can cache them until Cargo manifests change.
# As before, use cache mounts for better perf.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo chef cook --locked --release --recipe-path recipe.json

COPY . .

# Always use `--locked` to use the exact versions of dependencies specified in `Cargo.lock`.
# And again, cache mounts for better perf.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo build --locked --release --bin nv-server

# ================================================================================================
# Runtime Layer
# ------------------------------------------------------------------------------------------------

FROM debian:bookworm-slim AS runtime

ARG IMAGE_CREATED
ARG IMAGE_REVISION
ARG IMAGE_SOURCE
ARG IMAGE_VERSION

LABEL org.opencontainers.image.title="nv-server" \
    org.opencontainers.image.description="Night Vision backend server" \
    org.opencontainers.image.source="${IMAGE_SOURCE}" \
    org.opencontainers.image.version="${IMAGE_VERSION}" \
    org.opencontainers.image.revision="${IMAGE_REVISION}" \
    org.opencontainers.image.created="${IMAGE_CREATED}" \
    org.opencontainers.image.vendor="The MITRE Corporation" \
    org.opencontainers.image.licenses="Apache-2.0"

# Suppress prompts in `apt-get` since we're running under automation.
ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --uid 10001 --home-dir /nonexistent --shell /usr/sbin/nologin nv-server \
    && mkdir -p /etc/night-vision

COPY --from=builder /usr/src/night-vision/target/release/nv-server /usr/local/bin/nv-server
COPY deploy/nv-server.spookey /etc/night-vision/nv-server.spookey

RUN chown -R nv-server:nv-server /etc/night-vision
USER nv-server
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/nv-server"]
CMD ["--config", "/etc/night-vision/nv-server.spookey"]
