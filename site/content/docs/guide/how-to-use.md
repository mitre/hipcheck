---
title: How to Use Hipcheck
---

# How to Use Hipcheck

If you are interested in a quick guide to getting started with Hipcheck,
we recommend checking out the [Quickstart guide](@/docs/quickstart/_index.md)
first! For a more thorough explanation of Hipcheck's Command Line Interface
(CLI), please continue with this section!

---

Hipcheck's Command Line Interface features a number of different commands
for analyzing software packages (`hc check`), understanding the functioning
of Hipcheck (`hc schema`, `hc ready`), and managing Hipcheck itself
(`hc setup`, `hc update`). In this section, we'll walk through each of the
commands, describe their interface, what they're used for, and how to make
the most of them.

- [Subcommands](#subcommands)
  - [`hc check`](#hc-check)
  - [`hc ready`](#hc-ready)
  - [`hc schema`](#hc-schema)
  - [`hc setup`](#hc-setup)
  - [`hc update`](#hc-update)
- [General Flags](#general-flags)
  - [Output Flags](#output-flags)
  - [Path Flags](#path-flags)
  - [Help and Version](#help-and-version)

## Subcommands

### `hc check`

`hc check` is the primary command that user's of Hipcheck will run. It's the
command for running analyses of target packages (for more information on how
to specify "targets," see [the "Targets" documentation][target]).

The short help text for `hc check` looks like this:

```
Analyze a package, source repository, SBOM, or pull request

Usage: hc check [OPTIONS] <TARGET>

Arguments:
  <TARGET>  The target package, URL, commit, etc. for Hipcheck to analyze. If ambiguous, the -t flag must be set

Options:
  -t, --target <TARGET_TYPE>  [possible values: maven, npm, pypi, repo, request, spdx]
  -h, --help                  Print help (see more with '--help')

Output Flags:
  -v, --verbosity <VERBOSITY>  How verbose to be [possible values: quiet, normal]
  -k, --color <COLOR>          When to use color [possible values: always, never, auto]
  -f, --format <FORMAT>        What format to use [possible values: json, human]

Path Flags:
  -c, --config <CONFIG>  Path to the configuration folder
  -d, --data <DATA>      Path to the data folder
  -C, --cache <CACHE>    Path to the cache folder
```

The only positional argument is the `<TARGET>`, as explains in [the Targets
documentation][target]. This argument is _required_, and tells Hipcheck what to
analyze.

It is possible for a target specifier to be ambiguous. For example, Hipcheck
accepts targets of the form `<package_name>[@<package_version>]`. In this case,
it's not clear from the target specifier what package host this package is
supposed to be hosted on. In these ambiguous cases, the user needs to specify
the __target type__ with the `-t`/`--type` flag. The full list of current types
is:

- `maven`: A package on Maven Central
- `npm`: A package on NPM
- `pypi`: A package on PyPI
- `repo`: A Git repository
- `spdx`: An SPDX document

If you attempt to run `hc check` with an ambiguous target specifier, Hipcheck
will produce an error telling you to use the `-t`/`--target` flag to manually
specify the target type.

Besides this flag, all other flags are general flags which Hipcheck accepts
for every command. See [General Flags](#general-flags) for more information.

[target]: @/docs/guide/concepts/index.md#targets

### `hc ready`

`hc ready` is a command for checking that Hipcheck is ready to run analyses.
This is intended to help the user debug issues with a Hipcheck installation,
including problems like missing configuration files, inaccessible config,
data, or cache paths, missing authentication tokens, and more.

`hc ready` has no special flags currently, and only accepts the
[General Flags](#general-flags) that _all_ Hipcheck commands accept.

The output of `hc ready` is a report containing key information about
Hipcheck, the third-party tools it relies on as external data sources,
the paths it's currently using for configuration files, data files,
and local repository clones, along with any API tokens it will use
for external API access.

If all required information is found and passes requirements for Hipcheck
to run, it will report that Hipcheck is ready to run.

We recommend running `hc ready` before running Hipcheck the first time,
and as a good first debugging step if Hipcheck begins reporting issues.

### `hc schema`

The `hc schema` command is intended to help users of Hipcheck who are trying
to integrate Hipcheck into other tools and systems. Hipcheck supports a JSON
output format for analyses, and `hc schema` produces a JSON schema description
of that output.

`hc schema` takes the name of the target type for which to print the schema.
For the list of target types, see [the documentation for the `hc check` command](#hc-check).

`hc schema` also takes the usual [General Flags](#general-flags).

### `hc setup`

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
see the documentation on Hipcheck's [Path Flags](#path-flags).

`hc setup` also supports Hipcheck's [General Flags](#general-flags).

### `hc update`

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

This command also supports Hipcheck's [General Flags](#general-flags), though
they are ignored.

## General Flags

There are three categories of flags which Hipcheck supports on all subcommands,
output flags, path flags, and the help and version flags (which actually
operate like subcommands themselves).

### Output Flags

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

### Path Flags

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

### Help and Version

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

{{ button(link="@/docs/guide/analyses.md", text="Analyses") }}
