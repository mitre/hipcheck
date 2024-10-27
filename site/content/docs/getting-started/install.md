---
title: Install Hipcheck
aliases:
  - "/install/_index.md"
weight: 1
---

# Install Hipcheck

This page covers different mechanisms for installing Hipcheck. It's organized
between [direct installation](#installing-directly) and
[container-based installation](#using-in-a-container).

## Installing Directly

Use the following instructions if you want to install Hipcheck onto your local
system _outside_ of a container. If you want to install Hipcheck _inside_ of a
container, see the [container installation instructions](#using-in-a-container).

### Install from Script

The easiest way to install Hipcheck is to use an install script included with
each release.

{{ install() }}

{% info(title="Setting up post-install") %}
After running the install script, run `hc setup` to set up your local
configuration and script files so Hipcheck can run.
{% end %}

We currently provide prebuilt binaries for the following targets:

- x64 Linux: `x86_64-unknown-linux-gnu`
- x64 Windows: `x86_64-pc-windows-msvc`
- Apple Silicon macOS: `aarch64-apple-darwin`
- Intel macOS: `x86_64-apple-darwin`

We provide installation shell scripts for:

- POSIX-compliant shells: recommended on Linux, macOS, and in the Windows
  Subsystem for Linux (WSL) on Windows.
- PowerShell: recommended on Windows.

These scripts install the Hipcheck binary and the Hipcheck self-updater.

{% info(title="Install Script Security") %}
Some users may be uncertain about using an install script piped into a shell
command. For those users, you can download each install script by hand before
running it. The scripts are included in the set of artifacts bundled with each
Hipcheck release.

The scripts themselves are only downloaded over TLS-protected connections, and
the artifacts are checked against SHA-256 hashes also included with each
release to ensure artifact integrity.
{% end %}

### Install from Source

If you're on a platform for which Hipcheck does not provide pre-built
binaries, or want to modify Hipcheck's default release build in some way, you
will need to build Hipcheck from source.

To build Hipcheck from source, you'll need:

- A Rust toolchain: see the [official Rust installation instructions](https://www.rust-lang.org/tools/install)
- The `protoc` compiler: see the [installation instructions](https://grpc.io/docs/protoc-installation/)

If you _only_ want to build from source without configuring the build in any
way, you can use `cargo install` to install Hipcheck into a Cargo0-specific
binary directory with the source found from Crates.io.

```sh
$ cargo install hipcheck
```

If you do want to build from source _and_ configure the build or modify
Hipcheck itself before building, you'll also need:

- Git: see the [official Git installation instructions](https://git-scm.com/downloads)

You can then clone the Hipcheck repository with Git, make whatever build
modifications you want to make, and then install with:

```sh
$ git clone https://github.com/mitre/hipcheck
$ cd hipcheck
$ cargo install --path hipcheck
```

{% info(title="Setting up post-install") %}
After running the install script, run `hc setup` to set up your local
configuration and script files so Hipcheck can run.
{% end %}

This will install the `hc` binary into your Cargo-specific binary
directory.


{% info(title="Disadvantages of this Approach") %}
Same as with the `cargo-binstall` installation, installing Hipcheck this way
_does_ work, but _does not_ install the Hipcheck self-updater, which the
recommended install scripts _do_ install. If you want Hipcheck to be able to
update itself to newer versions, you'll need to install it with an
[install script](#install-from-a-pre-built-script) or install the
self-updater yourself.
{% end %}

## Using in a Container

Hipcheck can also be used inside of a container in [Docker](https://www.docker.com/),
[Podman](https://podman.io/), and other container-based systems.

The Hipcheck project maintains an official `Containerfile` which describes
the Hipcheck container, and publishes images to Docker Hub with each new
release.

### Using Docker Hub

Hipcheck publishes container images to Docker Hub under the [`mitre/hipcheck`
namespace](https://hub.docker.com/r/mitre/hipcheck). In general, we maintain a
`latest` tag which _always_ refers to the most-recently published version, as
well as tags for each individual version.

You can use these with Docker by default, or with any other container system
which you have configured to be able to pull container images from Docker Hub.

For example, to run a short-lived container with Docker using the most recent
Hipcheck image, you might run:

```sh
$ docker run mitre/hipcheck:latest
```

### Using the `Containerfile`

You can also run Hipcheck from the local `Containerfile`, first by building
the image, and then by running that image. For example, with Docker:

To do this you will need Git to get a local copy of the repository, or to
download the repository contents from GitHub without the Git history.

```sh
$ git clone https://github.com/mitre/hipcheck
$ cd hipcheck
$ docker build -f dist/Containerfile .
```

This will build the image, which you can then use normally.

## Deprecated Methods

The following section lists any methods which were supported at one point
for prior versions of Hipcheck. They're recorded here in case you need to use
one of these older versions, but they are not recommended regardless, and
can't be used at all with newer versions.

### Install with `cargo-binstall`

{% warn(title="Deprecated in 3.8.0") %}
This method is deprecated as of version 3.8.0. You can use this method to
install old versions of Hipcheck, but it will not work for newer versions.

Read [RFD #7: Simplified Release Procedures](@/docs/rfds/0007-simplified-release-procedures.md)
to learn more.
{% end %}

Hipcheck is written in Rust, and releases of Hipcheck are published to
[Crates.io](https://crates.io), the official Rust open source package host.
The Rust ecosystem has a popular tool, called [`cargo-binstall`](https://github.com/cargo-bins/cargo-binstall)
that can search for and install prebuilt binaries for packages published
to Crates.io.

To install Hipcheck with `cargo-binstall`, you'll need:

- A Rust toolchain: see the [official Rust installation instructions](https://www.rust-lang.org/tools/install)
- `cargo-binstall`: see their [installation instructions](https://github.com/cargo-bins/cargo-binstall?tab=readme-ov-file#installation)

Then you can run:

```sh
$ cargo binstall hipcheck
```

{% info(title="Setting up post-install") %}
After running the install script, run `hc setup` to set up your local
configuration and script files so Hipcheck can run.
{% end %}

This will install the latest version of Hipcheck. To install a specific older
version instead, replace `hipcheck` with `hipcheck@<VERSION>`, replacing
`<VERSION>` with the version you need.

{% info(title="Disadvantages of this Approach") %}
Installing Hipcheck with `cargo-binstall` _does_ work, but _does not_ install
the Hipcheck self-updater, which the recommended install scripts _do_ install.
If you want Hipcheck to be able to update itself to newer versions, you'll need
to install it with an [install script](#install-from-a-pre-built-script) or
install the self-updater yourself.
{% end %}
