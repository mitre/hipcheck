# syntax=docker/dockerfile:1

# Build with:
#
#   docker build \
#     --secret id=ca_file,src="$HOME/.config/certs/system_certs.pem" \
#   
#     --build-arg HIPCHECK_REF=06a3db9394742a58a7fb3412b677db25feb6f678 \
#     --build-arg HIPCHECK_FETCH_EPOCH="$(date -u +%s)" \
#     --build-arg IMAGE_CREATED="$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
#     --build-arg IMAGE_REVISION="$(git rev-parse HEAD)" \
#     --build-arg IMAGE_VERSION="0.1.0" \
#     -f backend/Dockerfile \
#     -t nv-server:local \
#     backend

# Keep these versions synced with the versions set in `.flox/env/manifest.toml`.
ARG RUST_VERSION=1.95.0-bookworm
ARG CARGO_CHEF_VERSION=0.1.77
ARG HIPCHECK_RUST_VERSION=1.95.0-bookworm
ARG HIPCHECK_REF=06a3db9394742a58a7fb3412b677db25feb6f678
# Change the reviewed commit value to upgrade Hipcheck. HIPCHECK_FETCH_EPOCH
# allows CI to invalidate this layer without changing the pinned source.
ARG HIPCHECK_FETCH_EPOCH=0

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
# Use `--secret id=ca_file,src=<CA_FILE>` to pass the CA cert file or bundle into the
# container.
#
# See: https://docs.docker.com/build/building/secrets/#secret-mounts
RUN --mount=type=secret,id=ca_file,target=/tmp/build-ca.crt,required=false \
    # Use cache mounts to improve rebuild performance.
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    update-ca-certificates \
    && if [ -s /tmp/build-ca.crt ]; then \
        cat /tmp/build-ca.crt >> /etc/ssl/certs/ca-certificates.crt; \
    fi \
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
    cargo chef cook --locked --release --recipe-path recipe.json \
    && chown -R root:root target

COPY . .

# Always use `--locked` to use the exact versions of dependencies specified in `Cargo.lock`.
# And again, cache mounts for better perf.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo build --locked --release --bin nv-server

# ================================================================================================
# Hipcheck Artifact Layer
# ------------------------------------------------------------------------------------------------

# Hipcheck is built from Night Vision's internal fork. The selected ref is a
# build input; the resolved commit is written into the final image for
# provenance.
FROM rust:${HIPCHECK_RUST_VERSION} AS hipcheck-builder

ARG HIPCHECK_REF
ARG HIPCHECK_FETCH_EPOCH

RUN --mount=type=secret,id=ca_file,target=/tmp/build-ca.crt,required=false \
    --mount=type=secret,id=hipcheck_gitlab_token,required=true \
    apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates git mold pkg-config protobuf-compiler \
    && if [ -s /tmp/build-ca.crt ]; then \
        cat /tmp/build-ca.crt >> /etc/ssl/certs/ca-certificates.crt; \
    fi \
    && rm -rf /var/lib/apt/lists/* \
    && test -n "${HIPCHECK_FETCH_EPOCH}" \
    && git init /usr/src/hipcheck \
    && git -C /usr/src/hipcheck remote add origin https://github.com/mitre/hipcheck.git \
    && git -C /usr/src/hipcheck \
        -c "http.extraHeader=Authorization: Bearer $(cat /run/secrets/hipcheck_gitlab_token)" \
        fetch --depth 1 origin "${HIPCHECK_REF}" \
    && git -C /usr/src/hipcheck checkout --detach FETCH_HEAD \
    && git -C /usr/src/hipcheck rev-parse HEAD > /usr/src/hipcheck/REVISION

WORKDIR /usr/src/hipcheck

# The MVP policy uses only the MITRE binary-analysis plugin. Both executables
# are compiled from the same immutable Hipcheck source revision.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo build --locked --release --package hipcheck --package binary

# ================================================================================================
# Runtime Layer
# ------------------------------------------------------------------------------------------------

FROM debian:bookworm-slim AS runtime

ARG IMAGE_CREATED
ARG IMAGE_REVISION
ARG IMAGE_SOURCE
ARG IMAGE_VERSION
ARG HIPCHECK_REF

LABEL org.opencontainers.image.title="nv-server" \
    org.opencontainers.image.description="Night Vision backend server" \
    org.opencontainers.image.source="${IMAGE_SOURCE}" \
    org.opencontainers.image.version="${IMAGE_VERSION}" \
    org.opencontainers.image.revision="${IMAGE_REVISION}" \
    org.opencontainers.image.created="${IMAGE_CREATED}" \
    org.mitre.night-vision.hipcheck.ref="${HIPCHECK_REF}" \
    org.opencontainers.image.vendor="The MITRE Corporation" \
    org.opencontainers.image.licenses="Apache-2.0"

# Suppress prompts in `apt-get` since we're running under automation.
ENV DEBIAN_FRONTEND=noninteractive

# Git is a runtime dependency of nv-server's CVE List ingest path. Use the
# Debian Bookworm package here; the newer Flox Git is for team development
# workflows, not for this container runtime.
RUN --mount=type=secret,id=ca_file,target=/tmp/build-ca.crt,required=false \
    apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates curl git nodejs npm \
    && if [ -s /tmp/build-ca.crt ]; then \
        cat /tmp/build-ca.crt >> /etc/ssl/certs/ca-certificates.crt; \
    fi \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --uid 10001 --home-dir /nonexistent --shell /usr/sbin/nologin nv-server \
    && mkdir -p /etc/night-vision /opt/night-vision/hipcheck /var/cache/night-vision/hipcheck /var/lib/night-vision/hipcheck

ENV TMPDIR=/var/cache/night-vision/hipcheck/tmp

COPY --from=builder /usr/src/night-vision/target/release/nv-server /usr/local/bin/nv-server
COPY --from=hipcheck-builder /usr/src/hipcheck/target/release/hc /usr/local/bin/hc
COPY --from=hipcheck-builder /usr/src/hipcheck/target/release/binary /opt/night-vision/hipcheck/target/release/binary
COPY --from=hipcheck-builder /usr/src/hipcheck/target/release/binary /var/cache/night-vision/hipcheck/target/release/binary
COPY --from=hipcheck-builder /usr/src/hipcheck/REVISION /opt/night-vision/hipcheck/REVISION
COPY deploy/nv-server.spookey /etc/night-vision/nv-server.spookey
COPY deploy/nv-server-entrypoint.sh /usr/local/bin/nv-server-entrypoint
COPY hipcheck/config /opt/night-vision/hipcheck/config
COPY hipcheck/plugins /opt/night-vision/hipcheck/plugins
COPY hipcheck/plugins /var/cache/night-vision/hipcheck/plugins

RUN chmod 755 /usr/local/bin/nv-server-entrypoint \
    && chown -R nv-server:nv-server /etc/night-vision /opt/night-vision /var/cache/night-vision /var/lib/night-vision
# Hipcheck atomically moves plugin state into its cache. Keep its working
# directory on that same filesystem when Compose mounts the cache as a volume.
WORKDIR /var/cache/night-vision/hipcheck
USER nv-server
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/nv-server-entrypoint"]
CMD ["--config", "/etc/night-vision/nv-server.spookey"]
