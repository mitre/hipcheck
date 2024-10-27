---
title: hc schema
extra:
  nav_title: "<code>hc schema</code>"
---

# `hc schema`

The `hc schema` command is intended to help users of Hipcheck who are trying
to integrate Hipcheck into other tools and systems. Hipcheck supports a JSON
output format for analyses, and `hc schema` produces a JSON schema description
of that output.

`hc schema` takes the name of the target type for which to print the schema.
For the list of target types, see [the documentation for the `hc check` command](@/docs/guide/cli/hc-check.md).

`hc schema` also takes the usual [General Flags](@/docs/guide/cli/general-flags.md).
