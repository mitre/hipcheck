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
`hc` binary, not these additional configuration files. `hc setup` identifies
the correct location in your system for configuration files and writes the
files to that directory.

It also produces an export command that should be used to set the `HC_CONFIG`
environmental variable to the relevant directory, as necessary to run `hc check`.

`hc setup` supports Hipcheck's [General Flags](@/docs/guide/cli/general-flags.md).
