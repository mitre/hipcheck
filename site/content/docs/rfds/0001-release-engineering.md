---
title: Hipcheck Release Engineering
weight: 1
slug: 0001
extra:
  rfd: 1
  primary_author: Andrew Lilley Brinker
  primary_author_link: https://github.com/alilleybrinker
  status: Accepted
  pr: 48
---

# Hipcheck Release Engineering

One of the key questions for any piece of software is how new versions will be
created and distributed out to end-users.

Currently, Hipcheck basically handles distribution through the Git repository
itself, and an `install.sh` script. The script clones the Hipcheck repository,
builds Hipcheck from source with a Rust toolchain, and then places the
Hipcheck binary and supporting files into the proper locations. This _works_,
but isn't ideal for the following reasons:

- It requires the user to have a number of dependencies on their system,
  including Git and a Rust toolchain.
- It builds from scratch, which is slow and tedious and can lead to poor error
  messages.
- Building from the Git repository means users may get broken builds if
  something slips through CI.

Ideally, we'd have a release process which addresses those problems. To
accomplish this, I'd like to propose the following:

- Cut releases, with source published to Crates.io and both source and prebuilt
  binaries published through GitHub Actions with `cargo-dist`
- Use `cargo-release` to handle the actual workflow of this publication
  process.
- Use `git-cliff` to generate a `CHANGELOG.md` that reflects changes in each
  new version.
- Remove the `install.sh` script currently found in the root of the Hipcheck
  repository.

One complication with the above process is that by default, `cargo-dist` only
distributes the binaries it builds plus some minimal additional metadata. We
_might_ be able to get it working to also install our additional required
files, but this is probably not ideal.

Instead, I propose that we modify Hipcheck itself to handle local setup _after_
the binary has been installed. So instead of needing to set up the
configuration files and helper scripts alongside the binary as a pre-requisite
for running the binary, the binary itself can install those materials in the
same way the `install.sh` script currently does, and also provide some user
convenience validation tools to check the readiness of the local install.

In this way, Hipcheck could be installed as a standalone binary, and users
could then be instructed to run a "setup" command to finish installation.

## The Setup Process

So, what would this process look like? I propose the following two commands:

- `hc setup new`: This command would set up Hipcheck's configuration files and
  helper scripts in the appropriate location, similar to what the `install.sh`
  script does today.
- `hc setup check`: This command would validate Hipcheck's setup by checking
  for the presence of the configuration files, helper scripts, and third-party
  commands that it needs.

With these commands in place, users would only need to use the install script
produced by `cargo-dist`, or build it from source, or even install with
`cargo-binstall`, to get the `hc` binary; then they could run `hc setup new`,
and Hipcheck would be ready to run on their machine!
