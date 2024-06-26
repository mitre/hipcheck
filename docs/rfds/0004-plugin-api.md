
# Plugin API

While [RFD #3][rfd_3] addressed the overall vision for how the planned plugin
architecture for Hipcheck will work, the intent of this API is to sketch out
in greater detail what the specific API will be. This will still not be at
the level of detail of a full _implementation_ of that API in Hipcheck, but
will be at a level of detail sufficient for the Hipcheck team to ensure
alignment on _what_ we're trying to do, _how_ we're trying to do it, and the
decomposition of those parts necessary for execution so we can actually get
to work on building.

## Hipcheck's Flow, Reviewed

When a user runs Hipcheck today, there are several phases to execution, and
several "flows" for information. The phases are:

- __Target Resolution__, where Hipcheck determines _what_ the user is trying
  to analyze (the "target"). This is generally either a Git repository, which
  may be associated with a remote host like a GitHub repository, or a package
  on a known package host like NPM, PyPI, or Maven. If given a package host, for
  example as a package name and version number, Hipcheck investigates metadata
  for that package to also identify any associated Git repository. It's
  worth noting that today Hipcheck is limited only to Git repositories, and does
  not support analyzing metadata from other VCS systems like Mercurial, Sapling,
  Darcs, Pijul, or others.
- __Configuration Loading__, where Hipcheck loads the user's configuration files,
  parses them, reports errors and halts if any are found, and then proceeds.
- __Repository Cloning or Pulling__, where Hipcheck retrieves a local copy of
  the Git repository being analyzed, as Hipcheck assumes all analyses at
  minimum desire to investigate historical metadata associated with the target
  project's development. Hipcheck maintains a local cache of cloned repositories,
  and if an already-cloned repository is targeted for analysis will pull from
  that repository rather than cloning a fresh copy.
- __Analysis__, where Hipcheck runs any active analyses based on the user's
  configuration files. As analyses run, they produce one of three results:
  success, failure, or "errored." An "errored" analysis is one that failed to
  run to completion, and which produces an error message. Successful analyses
  mean that the analysis is considered benign and does _not_ contribute to a
  higher risk score; a failed analysis means that the analysis identified some
  concerning issues and _does_ contribute to a higher risk score. When analyses
  run, their results are cached in memory by Hipcheck's incremental computation
  system, which is designed to ensure that Hipcheck does not compute expensive
  things twice. For example, both the "church" and "entropy" analyses rely on
  Git diffs, which are expensive to compute, for _all_ commits in the history
  of a project. Today, the scheduling of analyses is single-threaded, and much
  of the execution of individual analyses is _also_ single-threaded, though we
  are working to change that. We also do not currently cache any data on disk,
  meaning we may be subject to memory pressure, especially when analyzing
  targets with large amounts of Git history. Analyses, besides their main
  outcome, may additionally report "concerns," which are additional pieces of
  guidance for end-users intended to help assist them in any manual evaluation
  of a target project they may undertake based on Hipcheck's overall
  recommendation.
- __Scoring__: Hipcheck takes the results (pass or fail) of each analysis,
  along with the weight provided for each analysis, and uses them to produce
  an overall score which is compared to the user's configured risk threshold.
  If the overall score ("risk score") is higher than the user's risk threshold,
  then Hipcheck makes an "investigate" recommendation to the user, indicating
  the user should manually review the target project for possible supply chain
  security risk.

There is a bit more to say on scoring. Hipcheck's analyses currently form a
predefined tree, with analysis organized into categories. At each level of the
tree, the nodes can have weights associated which influence their impact on
the overall score. At each level, all the siblings nodes of a particular parent
have weights which can be converted into percentages by summing all of them
together, then dividing each by the sum. Then, for each leaf (the actual
analyses run by Hipcheck), the percentage influence that analysis has on the
overall risk score is found by multiple each of its ancestor percentages
together.

This can be shown visually. The following a weighted score tree with all
default weights, `1` everywhere.

```
                                +--------------+
                                | Analysis - 1 |
                                +--------------+
                                |              |
                +---------------+              +-------------+
                | Practices - 1 |              | Attacks - 1 |
                +---+---+---+---+              +-------------+
                |   |   |   |   |              |             |
 +--------------+   |   |   |   |              |             +------------+
 | Activity - 1 |   |   |   |   |              |             | Commit - 1 |
 +--------------+   |   |   |   |              |             +-----+------+
    +---------------+   |   |   |              +----------+  |     |      |
    | Binary - 1    |   |   |   |              | Typo - 1 |  |     |      +-------------+
    +---------------+   |   |   |              +----------+  |     |      | Entropy - 1 |
            +-----------+   |   |                            |     |      +-------------+
            | Fuzz - 1  |   |   |                            |     +------------+
            +-----------+   |   |                            |     | Churn - 1  |
             +--------------+   |                            |     +------------+
             | Identity - 1 |   |                            +-----------------+
             +--------------+   |                            | Affiliation - 1 |
                   +------------+                            +-----------------+
                   | Review - 1 |
                   +------------+
```

Then, a tree with the weights evaluated into per-level percentages:

```
                                    +-----------------+
                                    | Analysis - 100% |
                                    +-----------------+
                                    |                 |
                  +-----------------+                 +---------------+
                  | Practices - 50% |                 | Attacks - 50% |
                  +---+---+---+-----+                 +---------------+
                  |   |   |   |     |                 |               |
 +----------------+   |   |   |     |                 |               +--------------+
 | Activity - 20% |   |   |   |     |                 |               | Commit - 50% |
 +----------------+   |   |   |     |                 |               +------+-------+
      +---------------+   |   |     |                 +------------+  |      |       |
      | Binary - 20%  |   |   |     |                 | Typo - 50% |  |      |       +------------------+
      +---------------+   |   |     |                 +------------+  |      |       | Entropy - 33.33% |
            +-------------+   |     |                                 |      |       +------------------+
            | Fuzz - 20%  |   |     |                                 |      +----------------+
            +-------------+   |     |                                 |      | Churn - 33.33% |
             +----------------+     |                                 |      +----------------+
             | Identity - 20% |     |                                 +----------------------+
             +----------------+     |                                 | Affiliation - 33.33% |
                     +--------------+                                 +----------------------+
                     | Review - 20% |
                     +--------------+
```

Finally, a tree with the leaf weights evaluated to the actual
percentages each individual analysis contributes to the overall
risk score:

```
                              +----------+
                              | Analysis |
                              +----------+
                              |          |
                  +-----------+          +--------------+
                  | Practices |          |    Attacks   |
                  +--+--+--+--+          +--------------+
                  |  |  |  |  |          |              |
 +----------------+  |  |  |  |          |              +--------+
 | Activity - 10% |  |  |  |  |          |              | Commit |
 +----------------+  |  |  |  |          |              +---+----+
     +---------------+  |  |  |          +------------+ |   |    |
     | Binary - 10%  |  |  |  |          | Typo - 25% | |   |    +------------------+
     +---------------+  |  |  |          +------------+ |   |    | Entropy - 16.65% |
         +--------------+  |  |                         |   |    +------------------+
         |  Fuzz - 10%  |  |  |                         |   +----------------+
         +--------------+  |  |                         |   | Churn - 16.65% |
          +----------------+  |                         |   +----------------+
          | Identity - 10% |  |                         +----------------------+
          +----------------+  |                         | Affiliation - 16.65% |
               +--------------+                         +----------------------+
               | Review - 10% |
               +--------------+
```

In the plugin system, the structure of this scoring tree will be
configurable by the user (and thus, dynamic instead of being static as
it is today), with users controlling how weights are assigned and how nesting
influences overall score.

This leads us directly into...

## Plugin Selection

In the planned plugin architecture, Hipcheck users will specify plugins
they want to use in a specific "run" of Hipcheck (a single execution of
the `hc` program) by a configuration file. While today Hipcheck's
configuration is done via a set of TOML files with a statically-defined
structure, configuration in the plugin system will be necessity be
dynamic, to reflect the user's ability to select what plugins to run, how to
configure those plugins, and how those plugins should interact with _scoring_,
as we saw in the last section.

Plugins will be specified by users with a mechanism for acquiring plugins,
which Hipcheck will store in a local plugins directly. The mechanism for
acquiring plugins is left unspecified here as it's not directly part of
the plugin API.

That said, plugins _will be_ expected to be executable on the user's
current operating system and architecture. This may mean precompiled
binaries compatible with the current target, or plugins provided in
scripting languages which can run on the current host, or plugins
wrapped in container images which handle necessary virtualization,
sandboxing, and availability of required plugin runtime dependencies
besides Hipcheck.

The specific syntax and format of this configuration is left to be
defined by another RFD.

## Plugin Loading API

The first interaction Hipcheck will have with plugins will be for loading
plugins which the user has requested. Plugins will be loaded via a
Command Line Interface (CLI) by which Hipcheck will:

1. Start each plugin as its own process.
2. Specify, via a `--port` flag, what port the process should bind to
   for communication with `hc`.

Initially, Hipcheck's interaction with plugins will be handled via local
[gRPC][grpc]. In the future, other interfaces may be added including
WebAssembly, which offers better opportunities for sandboxing. It's worth
describing in greater detail why Hipcheck is choosing to prioritize gRPC
in the design of its plugin system.

### Aside: Why gRPC?

gRPC is a well-known and generally mature system for inter-process
communication. It includes clients in many different languages, and
well-understood mechanisms for important features like deadlines,
compression, cancellation, error handling, and more. Protocol Buffers,
the encoding gRPC uses for RPC call information over-the-wire, is
similarly well-understood. Hipcheck's transition to a plugin system is
a substantial change for the tool and a risk to the viability of the
project in the long-term. Getting it right is a necessity to deliver
on Hipcheck's vision of a more secure open source software ecosystem,
and choosing gRPC is a way we are choosing to reduce technical risk
associated with the re-architecture.

### Back to Plugin Loading

Plugins are expected to generally communicate with Hipcheck over the
gRPC interface exclusively. `stdout` and `stderr` should be used for
logging only, and Hipcheck will handle redirecting those streams to
the appropriate locations based on how `hc` itself was called.

## Expected gRPC Calls for Plugins

With all of this now explained, we can start to break down the RPC calls defined between the `hc` binary and each of the running plugins.

## The `Plugin` Trait

```rust
trait Plugin {

}

```



[rfd_3]: ./003-plugin-architecture-vision.md
[grpc]: https://grpc.io/
