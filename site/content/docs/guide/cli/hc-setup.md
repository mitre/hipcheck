---
title: hc setup
extra:
  nav_title: "<code>hc setup</code>"
---

# `hc setup`

The `hc setup` command is intended to be run after first installing Hipcheck,
and again after updating Hipcheck, to ensure you have the required configuration
and data files needed for Hipcheck to run.

When installing Hipcheck, regardless of method, you are only installing the
`hc` binary, not these additional files. `hc setup` gathers those files for you,
and installs them into the appropriate locations.

If Hipcheck has been installed with the recommended install scripts included
with each release, then the correct configuration and data files for each
version are included with the bundle downloaded by that script. In that case,
`hc setup` will attempt to find those files locally and copy them into the
configuration and data directories.

If Hipcheck was installed via another method, or the files can't be found,
then `hc setup` will attempt to download them from the appropriate Hipcheck
release. Users can pass the `-o`/`--offline` flag to ensure `hc setup` does
_not_ use the network to download materials, in which case `hc setup` will
fail if the files can't be found locally.

The installation directories for the configuration and data files are
specified in the way they're normally specified. For more information,
see the documentation on Hipcheck's [Path Flags](@/docs/guide/cli/general-flags.md#path-flags).

`hc setup` also supports Hipcheck's [General Flags](@/docs/guide/cli/general-flags.md).
