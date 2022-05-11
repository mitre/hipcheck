#============================================================================
# Builder Layer

# Use a slim Rust/Alpine image for build tooling.
FROM rust:1.57.0-alpine3.14 as builder

# Set the working directory.
WORKDIR /build

# Copy the files we need.
#
# Unfortunately, to preserve folder structure, these need to be separated.
#
# 1) Our Cargo configuration.
# 2) The main Hipcheck crate.
# 3) Hipcheck's internal libraries.
# 4) The current Crate manifest and lockfile.
COPY .cargo/ .cargo/
COPY hipcheck/ hipcheck/
COPY libs/ libs/
COPY Cargo.toml Cargo.lock ./

# Prep the system.
#
# 1) -e:          Stop if any line errors,
#    -u:          Consider unset variables as an error when substituting,
#    -x:          Print commands and their arguments as they're executed,
#    -o pipefail: Pipelines return the status of the last command to exit
#                 with a non-zero status, or zero.
# 2) Setup the packages we'll need for our build:
#    - musl-dev:    Needed to build some C code we rely on.
#    - openssl-dev: Needed to build some networking code we have.
# 3) Delete `xtask/` from Cargo.toml so we can build without copying it.
# 4) Build Hipcheck in release configuration.
#    NOTE: The RUSTFLAGS are there to make sure musl is linked to dynamically,
#          because part of Hipcheck uses OpenSSL which links to it dynamically,
#          and combining a statically-linked and a dynamically-linked instance
#          of musl causes Hipcheck to segfault.
#
#    LINK: https://users.rust-lang.org/t/sigsegv-with-program-linked-against-openssl-in-an-alpine-container/52172/4
RUN set -eux -o pipefail; \
    apk add --no-cache musl-dev openssl-dev; \
    sed -i "/xtask\/*/d" Cargo.toml; \
    RUSTFLAGS="-C target-feature=-crt-static" cargo build --release

#============================================================================
# App Layer

# Use a slim Alpine image so our final container is small.
FROM alpine:3.14 as app

# Set the working directory.
WORKDIR /app

# Copy everything we need.
#
# 1) The Hipcheck binary.
# 2) The Hipcheck configuration.
# 3) The Hipcheck scripts.
COPY --from=builder /build/.target/release/hc ./hc
COPY config/ config/
COPY scripts/ scripts/

# Install everything we need and setup a non-root user.
#
# 1) Configure the shell.
# 2) Setup the packages Hipcheck needs to run:
#    - npm:         Used by Hipcheck to analyze JavaScript code.
#    - git:         Used by Hipcheck to collect repository data.
# 3) Add a user `hc_user` which will be set to run Hipcheck.
RUN set -eux -o pipefail; \
    apk add --no-cache npm git; \
    npm install -g module-deps@6.2 --no-audit --no-fund; \
    adduser --disabled-password hc_user && chown -R hc_user /app

# Set this after everything else so the binary is owned by root,
# but run by a non-root user who also has the environment variables.
USER hc_user

# Tell Hipcheck where the configuration and script files are.
ENV HC_CONFIG=./config
ENV HC_DATA=./scripts

# Make the container run Hipcheck.
ENTRYPOINT ["./hc"]

# By default, print the help text.
CMD ["help"]

