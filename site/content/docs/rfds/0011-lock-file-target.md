---
title: Dependency and Lock Files as Targets
weight: 11
slug: 0011
extra:
  rfd: 11
  primary_author: Julian Lanson
  primary_author_link: https://github.com/j-lanson
  status: Accepted
  pr: 963
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

### Target Resolution Refactoring

The proposed refactor to support this change is to rename the existing `struct
TargetSeed` object to `SingleTargetSeed`, and create a new `enum TargetSeed`
with variants for `Single`, and `Multi` targets. This creates space for the
creation of `MultiTargetSeedKind` and `MultiTargetSeed` objects as shown below:

```rust
pub enum SingleTargetSeedKind {
    LocalRepo(LocalGitRepo),
    RemoteRepo(RemoteGitRepo),
    Package(Package),
    MavenPackage(MavenPackage),
    Sbom(Sbom),
}

pub struct SingleTargetSeed {
    pub kind: SingleTargetSeedKind,
    pub refspec: Option<String>,
    pub specifier: String,
}

pub enum MultiTargetSeedKind {
    CargoToml(PathBuf),
    GoMod(PathBuf),
    PackageJson(PathBuf),
}

pub struct MultiTargetSeed {
    pub kind: MultiTargetSeedKind,
    pub specifier: String,
}

pub enum TargetSeed {
    Single(SingleTargetSeed),
    Multi(MultiTargetSeed),
}
```

We will update `TargetResolver::resolve()` from taking a `TargetSeed` to a
`SingleTargetSeed`, to preserve the API that it can only return one `Target`
(whereas taking a `TargetSeed` in the above design would allow a
`MultiTargetSeed` that could produce more than one `Target`).

We can then add the following `impl` to `TargetSeed`:

```rust
impl TargetSeed {
    pub fn get_targets(config: TargetResolverConfig) -> impl Iterator<Item = Target> {
        use TargetSeed::*;
        match &self {
            Single(x) => vec![TargetResolver::resolve(config, x)?].into()
            // MultiTargetSeed variants have custom parsing but ultimately call
			// `TargetResolver::resolve` on each `SingleTargetSeed` they
			// generate.
            Multi(x) => todo!()
        }
    }
}
```

This API will guide the Hipcheck core code from its current design of expecting
`TargetSeed` to produce one `Target`, to a design that can handle a potentially
infinite stream of `Target` objects.

### Per-Target Controls on Multi-Target Seeds

There are some existing options on the `hc check` command that do not make sense
for a package file or other multi-target seed, such as the `--ref` flag (it is
extremely unlikely you would have the same target refspec for every repository
resolved).  Additionally, one can imagine the existene of flags that would make
sense for a multi-target seed and not for a single-target seed. With the current
CLI-parsing implementation in `cli.rs`, we may add a new `CheckLockFileArgs` (or
more appropriate named) struct with arguments specific to lock files, thus
handling the above issue of flags that are only appropriate for packages. To
ensure the `--ref` flag is not applied to multi-repo targets, we can mimic the
existing validation logic in `impl ToTargetSeed for CheckArgs` that ensures a
`Package` target seed does not have both a refspec and a version identifier.

It is tempting to consider adding flags to `CheckLockFileArgs` to support
fine-grained control over the dependencies within a multi-target seed.  For
example, a user might desire to apply different policy files or investigation
policy expressions for different dependencies within a package file. This could
easily arise in the imagined use-case of running Hipcheck as a step in a
project's Continuous Integration (CI) system; once flagged dependencies are
investigated, the user no longer wants the "investigate" determination to
prevent the CI step from passing, which they could do with a separate policy for
those failing dependencies. We considered the following possible designs
to support this: a separate multi-target-seed-specific subcommand with
additional flags for control, and a "dependency policy file" to control which
policy files get applied to which dependencies. **We are explicitly foreclosing
these approaches**, and sticking to a one-policy-per-execution design. In a
subsequent RFD on report caching, we describe in detail an approach to
addressing the "ignore already investigated target" CI issue. In brief, as
reports are cached, users can mark the report as having been reviewed so that
they are not reported on subsequent runs so long as the same `hc` binary,
policy, and target are used.

### Refspecs on Targets Inside Multi-Target Seeds

Although the CLI-level `--ref` flag does not apply to multi-target seeds, we
ought to ensure that any refspec-like information on the contained dependencies
is captured and communicated to the analysis subsystem of Hipcheck core. For
dependencies that are specified as package versions, the desired refspec is
implicitly captured in the version number. There are however other types of
target specifications, such as the [VCS URL][vcs-url] which embeds version
control ref information.

As part of this RFD, we propose to add the VCS URL specification as a new
`SingleTargetSeed` variant so that we can capture refspec information on
dependencies in package lock files that are specified as Git repositories.  We
anticipate this will be a necessary step for supporting `go.mod` files (which
specify dependencies as Git repositories) as multi-target seeds. This feature
will also be necessary to support `Cargo.toml` files that contain
Git-dependencies.

We would however like to ensure that `--ref`-like information specified for
given dependencies inside a package file or other multi-target seed can be
communicated to the analysis subsystem of Hipcheck core.

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

### Report Aggregation

Hipcheck should emit reports as they are generated by each async task or
subprocess, as opposed to aggregating them in memory. To maintain the same
user-experience as Hipcheck provides today, these reports should be emitted in
serial to the shell by default, in the format specified by the `--format` flag.
We may consider adding a `--format summary` option to the commandline that
prints simple `<TARGET>: PASS/INVESTIGATE` lines to improve the user-experience
for multi-target seeds. For failing targets, users could re-run Hipcheck on them
directly to get the full report. With the functionality proposed by the Report
Caching RFD, this would not require actually re-computing the analysis.

Some users experimenting with Hipcheck have expressed the desire for an
`--output` flag that could control where Hipcheck prints its output, such as
redirecting to a file. Adding support for this could be an initial step for
allowing the individual target reports produced by a multi-target seed to be
written to distinct files within a directory.

[vcs-url]: https://pip.pypa.io/en/stable/topics/vcs-support/
