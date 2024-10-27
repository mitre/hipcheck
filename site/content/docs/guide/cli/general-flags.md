---
title: General Flags
---

# General Flags

There are three categories of flags which Hipcheck supports on all subcommands,
output flags, path flags, and the help and version flags (which actually
operate like subcommands themselves).

## Output Flags

"Output flags" are flags which modify the output that Hipcheck produces.
Currently, there are three output flags:

- `-v <VERBOSITY>`/`--verbosity <VERBOSITY>`: Specifies how noisy Hipcheck
  should be when running. Options are:
  - `quiet`: Produce as little output as possible.
  - `normal`: Produce a normal amount of output. (default)
- `-k <COLOR>`/`--color <COLOR>`: Specifies whether the Hipcheck output should
  include color or not. Options are:
  - `always`: Try to produce color regardless of the output stream's support
    for color.
  - `never`: Do not produce color.
  - `auto`: Try to infer whether the output stream supports ANSI color codes.
    (default)
- `-f <FORMAT>`/`--format <FORMAT>`: Specifies what format to use for the
  output. Options are:
  - `json`: Use JSON output.
  - `human`: Use human-readable output. (default)

Each of these can also be set by environment variable:

- `HC_VERBOSITY`
- `HC_COLOR`
- `HC_FORMAT`

The precedence is, in increasing order:

- Environment variable
- CLI flag

## Path Flags

"Path flags" are flags which modify the paths Hipcheck uses for configuration,
data, and caching repositories locally. The current flags are:

- `-c <CONFIG>`/`--config <CONFIG>`: the path to the configuration folder to
  use.
- `-d <DATA>`/`--data <DATA>`: the path to the data folder to use.
- `-C <CACHE>`/`--cache <CACHE>`: the path to the cache folder to use.

Each of these is inferred by default based on the user's platform. They can
also be set with environment variables:

- `HC_CONFIG`
- `HC_DATA`
- `HC_CACHE`

The priority (in increasing precedence), is:

- System default
- Environment variable
- CLI flag

## Help and Version

All commands in Hipcheck also support help flags and the version flag.
These act more like subcommands, in that providing the flag stops Hipcheck
from executing the associated command, and instead prints the help or
version text as requested.

For each command, the `-h` or `--help` flag can be used. The `-h` flag gives
the "short" form of the help text, which is easier to skim, while the `--help`
flag gives the "long" form of the help text, which is more complete.

The `-V`/`--version` flag may also be used. Both the short and long variants
of the flag produce the same output. The version flag is valid on all
subcommands, but all subcommands are versioned together, to the output will
be the same when run as `hc --version` or when run as
`hc <SUBCOMMAND> --version`.
