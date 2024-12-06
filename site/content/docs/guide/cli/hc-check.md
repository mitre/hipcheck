---
title: hc check
extra:
  nav_title: "<code>hc check</code>"
---

# `hc check`

`hc check` is the primary command that users of Hipcheck will run. It's the
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
for every command. See [General Flags](@/docs/guide/cli/general-flags.md)
for more information.

[target]: @/docs/guide/concepts/targets.md
