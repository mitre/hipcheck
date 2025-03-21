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
      --ref <REF>             The ref (e.g. commit hash, branch, tag) of the target to analyze
      --arch <ARCH>
  -t, --target <TARGET_TYPE>  [possible values: maven, npm, pypi, repo, request, sbom]
  -h, --help                  Print help (see more with '--help')

Output Flags:
  -v, --verbosity <VERBOSITY>  How verbose to be [possible values: quiet, normal]
  -k, --color <COLOR>          When to use color [possible values: always, never, auto]
  -f, --format <FORMAT>        What format to use [possible values: json, debug, human]

Path Flags:
  -C, --cache <CACHE>    Path to the cache folder
  -p, --policy <POLICY>  Path to the policy file
  -e, --exec <EXEC>      Path to the exec config file
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

The `--ref` flag enables users to analyze a specific subset of a git repository's history.
A valid `--ref` is any git identifier that can be resolved to a specific commit.
Examples of git references include commit hashes, tags, remote tracking branches,
local branches and HEAD.

If the user wants to override the architecture detection feature of hipcheck,
which is used in plugin selection, then use the `--arch` flag. This flag
takes a target triple argument. In general, hipcheck uses the same target triples as
[those tracked by the Rust programming language project][target-triples]. Currently,
hipcheck supports the following target triples:

- `aarch64-apple-darwin` - MacOS running on aarch64
- `x86_64-apple-darwin` - MacOS running on x86_64
- `x86_64-unknown-linux-gnu` - Linux running on x86_64 with glibc 2.17+
- `x86_64-pc-windows-msvc` - Windows with Microsoft Visual C++ running on x86_64

Besides this flag, all other flags are general flags which Hipcheck accepts
for every command. See [General Flags](@/docs/guide/cli/general-flags.md)
for more information.

[target]: @/docs/guide/concepts/targets.md
[target-triples]: https://doc.rust-lang.org/beta/rustc/platform-support.html
