#============================================================================
# Builder Layer

# Use a slim Rust/Debian image for build tooling.
FROM rust:1.79.0-slim-bookworm AS builder

# Set the working directory.
WORKDIR /build

# Copy the files we need.
#
# Unfortunately, to preserve folder structure, these need to be separated.
COPY .cargo/ .cargo/
COPY hipcheck-macros/ hipcheck-macros/
COPY hipcheck/ hipcheck/
COPY xtask/ xtask/
COPY Cargo.toml Cargo.lock ./

# Prep the system.
#
# 1) -e:          Stop if any line errors,
#    -u:          Consider unset variables as an error when substituting,
#    -x:          Print commands and their arguments as they're executed,
#    -o pipefail: Pipelines return the status of the last command to exit
#                 with a non-zero status, or zero.
# 2) Setup the packages we'll need for our build:
#    - build-essential: includes make, to build openssl
#    - perl-base: perl is also needed to build openssl
# 3) Build Hipcheck in release configuration.
RUN set -eux; \
    apt-get install -y build-essential perl-base; \
    cargo build --release

#============================================================================
# App Layer

FROM debian:bookworm-slim AS app

# Set the working directory.
WORKDIR /app

# Copy everything we need.
#
# 1) The Hipcheck binary.
# 2) The Hipcheck configuration.
# 3) The Hipcheck scripts.
COPY --from=builder /build/target/release/hc ./hc
COPY config/ config/
COPY scripts/ scripts/

# Install everything we need and setup a non-root user.
#
# 1) Configure the shell.
# 2) Setup the packages Hipcheck needs to run:
#    - npm:         Used by Hipcheck to analyze JavaScript code.
#    - git:         Used by Hipcheck to collect repository data.
# 3) Add a user `hc_user` which will be set to run Hipcheck.
RUN set -eux; \
    apt-get install -y npm git; \
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
