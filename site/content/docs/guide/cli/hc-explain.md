---
title: hc explain
extra:
  nav_title: "<code>hc explain</code>"
---

# `hc explain`

The `hc explain` command is provided to give users insight into some of the
current settings, information, or logic to use for understanding or debugging.

The following is the CLI help text for `hc explain`:

```
View setup information to help debug

Usage: hc explain [OPTIONS] <COMMAND>

Commands:
  target-triple  Show the current and known architecture targets
  help           Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help (see more with '--help')

Output Flags:
  -v, --verbosity <VERBOSITY>  How verbose to be [possible values: quiet, normal]
  -k, --color <COLOR>          When to use color [possible values: always, never, auto]
  -f, --format <FORMAT>        What format to use [possible values: json, debug, human]

Path Flags:
  -C, --cache <CACHE>    Path to the cache folder
  -p, --policy <POLICY>  Path to the policy file
  -e, --exec <EXEC>      Path to the exec config file
```

## hc explain target-triple

The `target-triple` commands prints out the current inferred architecture of
the user as recognized by Hipcheck. It also prints out the other supported 
architectures. A sample output for `hc explain target-triple` would be:

```
Current target triple architecture (Known):
  > Apple Silicon (ARM64), macOS

Other supported architectures:
  - Intel x86_64, macOS
  - Intel x86_64, Windows (MSVC)
  - Intel x86_64, Linux (GNU)
  - ARM64, Linux
```
