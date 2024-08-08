#============================================================================
# Builder Layer

FROM rust:1.79.0-slim-bookworm AS builder

WORKDIR /build

COPY .cargo/ .cargo/
COPY hipcheck-macros/ hipcheck-macros/
COPY hipcheck/ hipcheck/
COPY xtask/ xtask/
COPY Cargo.toml Cargo.lock ./

RUN set -eux && \
    apt-get update && \
    apt-get install -y build-essential perl-base && \
    cargo build --release

#============================================================================
# App Layer

FROM debian:bookworm-slim AS app

WORKDIR /app

COPY --from=builder /build/target/release/hc ./hc
COPY config/ config/
COPY scripts/ scripts/

RUN set -eux && \
    apt-get update && \
    apt-get install -y npm git && \
    apt-get clean && \
    npm install -g module-deps@6.2 --no-audit --no-fund && \
    adduser --disabled-password hc_user && \
    chown -R hc_user /app

USER hc_user

ENV HC_CONFIG=./config
ENV HC_DATA=./scripts

ENTRYPOINT ["./hc"]

CMD ["help"]
