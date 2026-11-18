# syntax=docker/dockerfile:1

# Build with:
#
#   docker build \
#     --build-arg IMAGE_CREATED="$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
#     --build-arg IMAGE_REVISION="$(git rev-parse HEAD)" \
#     --build-arg IMAGE_VERSION="0.1.0" \
#     -f frontend/Dockerfile \
#     -t nv-app:local \
#     frontend

# Keep these versions synced with the versions set in `.flox/env/manifest.toml`.
ARG NODE_VERSION=24.15.0-bookworm-slim
ARG PNPM_VERSION=11.4.0

# OCI Image Metadata.
ARG IMAGE_CREATED
ARG IMAGE_REVISION
ARG IMAGE_SOURCE=https://github.com/mitre/hipcheck/night-vision
ARG IMAGE_VERSION=0.1.0

# ================================================================================================
# Base Layer
# ------------------------------------------------------------------------------------------------

FROM node:${NODE_VERSION} AS base

ARG PNPM_VERSION

ENV PNPM_HOME=/pnpm
ENV PATH="${PNPM_HOME}:${PATH}"

WORKDIR /usr/src/night-vision/frontend

RUN corepack enable \
    && corepack prepare "pnpm@${PNPM_VERSION}" --activate

# ================================================================================================
# Dependencies Layer
# ------------------------------------------------------------------------------------------------

FROM base AS dependencies

COPY package.json pnpm-lock.yaml pnpm-workspace.yaml .npmrc svelte.config.js ./

RUN --mount=type=cache,id=pnpm-store,target=/pnpm/store \
    pnpm install --frozen-lockfile

# ================================================================================================
# Builder Layer
# ------------------------------------------------------------------------------------------------

FROM base AS builder

COPY --from=dependencies /usr/src/night-vision/frontend/node_modules ./node_modules
COPY . .

RUN pnpm run build

# ================================================================================================
# Runtime Layer
# ------------------------------------------------------------------------------------------------

FROM node:${NODE_VERSION} AS runtime

ARG IMAGE_CREATED
ARG IMAGE_REVISION
ARG IMAGE_SOURCE
ARG IMAGE_VERSION

LABEL org.opencontainers.image.title="nv-app" \
    org.opencontainers.image.description="Night Vision frontend application" \
    org.opencontainers.image.source="${IMAGE_SOURCE}" \
    org.opencontainers.image.version="${IMAGE_VERSION}" \
    org.opencontainers.image.revision="${IMAGE_REVISION}" \
    org.opencontainers.image.created="${IMAGE_CREATED}" \
    org.opencontainers.image.vendor="The MITRE Corporation" \
    org.opencontainers.image.licenses="Apache-2.0"

ENV NODE_ENV=production
ENV HOST=0.0.0.0
ENV PORT=3000

WORKDIR /usr/src/night-vision/frontend

RUN useradd --uid 10002 --home-dir /nonexistent --shell /usr/sbin/nologin nv-app

COPY --from=builder --chown=nv-app:nv-app /usr/src/night-vision/frontend/build ./build

USER nv-app
EXPOSE 3000
ENTRYPOINT ["node", "build"]
