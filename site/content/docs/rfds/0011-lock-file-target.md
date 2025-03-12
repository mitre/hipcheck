---
title: Dependency and Lock Files as Targets
weight: 11
slug: 0011
extra:
  rfd: 11
  primary_author: Julian Lanson
  primary_author_link: https://github.com/j-lanson
  status: Proposed
---

# Dependency and Lock Files as Targets

Currently, Hipcheck operates on a single-target-per-execution basis. However,
this model has some friction with one aspect of Hipcheck's elevator pitch,
namely that you could run it against all the dependencies of your codebase.
Scripting or manually entering separate Hipcheck invocations for each of your
project's dependencies puts extra burden on would-be Hipcheck adopters.
Furthermore, it is computationally wasteful since each invocation would spin
up, configure, and tear down the same plugin processes that the next invocation
would also need. To address this, this RFD proposes to make project package and
lock files that specify dependencies into first-class analysis targets. To do
so, this document describes proposed changes to the internal API for `Session`
objects and the `TargetSeed` to `Target` resolution step so that a spun-up set
of plugins can be re-used for multiple `Target`s in series.

Since projects can have many dependencies, we also propose some parallelization
strategies. These include potential tweaks to enable multiple Hipcheck instances
to run simultaneously, or for a single instance to analyze multiple `Targets`
in parallel with `async` code, despite current restrictions caused by `salsa`.

## TargetSeed to Target Conversion Changes

One of the main API changes introduced by adding package and lock files as
target types is that we no longer can assume each target supplied to Hipcheck
resolves to only one repository (a single-repo target). A package file
containing a list of dependencies would instead correspond to zero or more
separate repositories (a multi-repo target).

### Command-line Interface Change Considerations

The simple proposed refactor to support this change is to first add new
`TargetSeedKind` variants to represent package/lock file types. As an example,
a user may invoke `hc` on the command-line as `hc check Cargo.toml` to
indicate Hipcheck should run against all dependencies in that file. To make
logic unique to single- or multi-repo targets easier to write, we probably
Gshould rename the existing `TargetSeedKind` to `SingleTargetSeed`, add a
separate `MultiTargetSeed` and then redefine `TargetSeedKind` as follows:

```rust
enum TargetSeedKind {
	Single(SingleTargetSeed)
	Multi(MultiTargetSeed)
}
```

There are some types of existing options that do not make sense for a package
file, such as the `--ref` flag (it is extremely unlikely you would have the same
target refspec for every repository resolved). Additionally, there are some
non-existent flags that would make sense for a package target and not for a
single-repository target; for example, a list of dependencies within the package
file on which to skip analysis.

With the current CLI-parsing implementation in `cli.rs`, we could add a new
`CheckLockFileArgs` (or more appropriate named) struct with arguments specific
to lock files, thus handling the above issue of flags that are only appropriate
for packages. To ensure the `--ref` flag is not applied to multi-repo targets,
we can do validation alongside the existing validation in
`impl ToTargetSeed for CheckArgs` that ensures a `Package` target seed does not
have both a refspec and a version identifier.

The above proposed design makes the assumption that users will want to apply the
same policy to all dependencies in a lock file. We can imagine a scenario where
in the first execution, Hipcheck finds a handful of dependencies that should be
investigated, but the user wants to ignore the warning or use a different policy
file or risk threshold for those dependencies. The ability to specify multiple
policies or slightly tweak the existing one from the CLI is something that the
`hc check` subcommand is not well-equipped for. We probably want to avoid having
to add global flags to the `hc check` subcommand that apply to some but not all
`TargetSeed` variants.  Therefore, as an backup design we may instead add a new
subcommand specifically for multi-repo targets.

### Internal Refactoring

With the above `TargetSeedKind` refactor, we will want to update
`TargetResolver::resolve()` to only run on instances of `SingleTargetSeed`. We
may then have a separate `TargetResolver::resolve_multi()` that takes a
`MultiTargetSeed` and returns `impl Iterator<Result<Target>>`. This design
allows a `MultiTargetSeed` to produce zero or more targets. As an example the
implementation for `MultiTargetSeed::LockFile` can return an iterator that
parses each dependency from its source file, turns each into a
`SingleTargetSeed`,

## Session Refactor

For a Hipcheck execution that analyzes multiple targets, keeping the plugins
alive between targets is important for efficiency, but we likely do not want any
Salsa memoization to persist between analyzes. The `HcCore` is our handle to all
plugins, and it is wrapped in an `Arc`, so we could safely clone it between
`Session` objects to keep the plugins live. Each `Session` has its own Salsa
database, so to reuse an existing `Session` we need to clear that database, or
we can choose to have a one-session-one-repo policy. The latter would be flexible
for allowing us one day to support different policy files against different
repos produced by a multi-repo target seed, since the policy file is also stored
within the session object.

A refactored Session object ready for parallelization should have the
following:
- A `storage` field to contains the `salsa` query storage.
- An `Arc<HcPluginCore>` handle to communicate with a set of plugins.
- A `Shell` handle it will use for all output. In parallelized contexts the
  Shell object can refrain from printing anything to output.
- A re-usable `fn run(target: Target, policy: Policy, clear_db: bool) ->
  JsonValue` that returns the raw JSON report.

With the above changes, the decision about output format is moved to the calling
function, so we can, for example, decide to create a summary (for multi-target
seeds) or a detailed human-readable report on the shell (for a single-target
seed).

## Parallelization

Naturally, if Hipcheck is running against tens or hundreds of repositories, it
will be slow to analyze them all in serial. The `salsa` crate we use for
memoization does not support asynchronous functions, however we likely don't
want memoziation to be shared between targets in the first place. Therefore, we
could imagine a design where we create a pool of threads or `tokio::Task`
objects that each get a separate target-less (see above) `Session` object on
initialization, then make synchronous calls to their own memoized `query()`
function, with the different tasks' output finally aggregating into a
`HashMap<(Target, Policy), Report>`. If we create a pool of tasks smaller than
the number of targets, the existing session in a task that gets a new target
must first clear its memoziation cache.

As an alternative to intra-process parallelization with async code, we could
spawn Hipcheck subprocesses that each communicate with their own copies of the
plugin processes.
