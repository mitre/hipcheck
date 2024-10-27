---
title: hc update
extra:
  nav_title: "<code>hc update</code>"
---

# `hc update`

When Hipcheck is installed using the recommend install scripts provided with
each release, the install scripts also provide an "updater" program, built
by `cargo-dist` (which Hipcheck uses to handle creating prebuilt artifacts
with each release, and with announcing each release on GitHub Releases).
This updater program handles checking for a newer version of Hipcheck,
downloading it, and replacing the current version with the newer one.
The updater is provided as a separate binary (named either `hc-update`
or `hipcheck-update`, due to historic bugs).

The `hc update` command simply delegates to this separate update program,
and provides the same interface that this separate update program does.
In general, you only need to run `hc update` with no arguments, followed
by `hc setup` to ensure you have the latest configuration and data files.

If you want to specifically download a version besides the most recent
version of Hipcheck, you can use the following flags:

- `--tag <TAG>`: install the version from this specific Git tag.
- `--version <VERSION>`: install the version from this specific GitHub
  release.
- `--prerelease`: permit installing a pre-release version when updating
  to `latest`,

The `--tag` and `--version` flags are mutually exclusive; the updater will
produce an error if both are provided.

This command also supports Hipcheck's [General Flags](@/docs/guide/cli/general-flags.md), though
they are ignored.
